use crate::filter_types::{CodeletType, Filter};
use crate::sim_element::SimElement;
use rpc_lib::rpc::Rpc;
use std::collections::HashMap;
use std::env;
use std::fmt;
use std::path::PathBuf;

pub struct PluginWrapper {
    // https://docs.rs/libloading/0.6.5/libloading/os/index.html
    // TODO: Currently uses a platform-specific binding, which isn't very safe.
    filter: Filter,
    loaded_function: libloading::os::unix::Symbol<CodeletType>,
    id: u32,
    stored_rpc: Option<Rpc>,
    neighbor: Option<u32>,
}

impl fmt::Display for PluginWrapper {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(width) = f.width() {
            write!(
                f,
                "{:width$}",
                &format!("PluginWrapper {{ id : {} }}", &self.id),
                width = width
            )
        } else {
            write!(f, "PluginWrapper {{ id : {} }}", &self.id)
        }
    }
}

impl SimElement for PluginWrapper {
    fn tick(&mut self, _tick: u64) -> Vec<(Rpc, Option<u32>)> {
        if self.stored_rpc.is_some() {
            let ret = self.execute(self.stored_rpc.as_ref().unwrap());
            self.stored_rpc = None;
            if ret.is_none() {
                vec![]
            } else {
                vec![(ret.unwrap(), self.neighbor)]
            }
        } else {
            vec![]
        }
    }
    fn recv(&mut self, rpc: Rpc, _tick: u64) {
        assert!(self.stored_rpc.is_none(), "Overwriting previous RPC");
        self.stored_rpc = Some(rpc);
    }
    fn add_connection(&mut self, neighbor: u32) {
        self.neighbor = Some(neighbor);
    }
}

impl PluginWrapper {
    pub fn new(plugin_str: &str, id: u32) -> PluginWrapper {
        let mut plugin_path = PathBuf::from(plugin_str);
        match env::consts::OS {
            "macos" => {
                plugin_path.set_extension("dylib");
            }
            "linux" => {
                plugin_path.set_extension("so");
            }
            _ => panic!("Unexpected operating system."),
        }
        // Load library with  RTLD_NODELETE | RTLD_NOW to avoid freeing the lib
        // https://github.com/nagisa/rust_libloading/issues/5#issuecomment-244195096
        let os_lib =
            libloading::os::unix::Library::open(plugin_path.to_str(), 0x2 | 0x1000).unwrap();
        let dyn_lib = libloading::Library::from(os_lib);

        // Dynamically load one function to initialize hash table in filter.
        let filter_init = unsafe {
            let tmp_loaded_function: libloading::Symbol<fn(HashMap<String, String>) -> Filter> =
                dyn_lib
                    .get("new_with_envoy_properties".as_bytes())
                    .expect("load symbol");
            tmp_loaded_function.into_raw()
        };

        // Dynamically load another function to execute filter functionality.
        let loaded_function = unsafe {
            let tmp_loaded_function: libloading::Symbol<CodeletType> =
                dyn_lib.get("execute".as_bytes()).expect("load symbol");
            tmp_loaded_function.into_raw()
        };

        // Put in envoy properties in the new filter
        let mut envoy_properties = HashMap::new();
        envoy_properties.insert(String::from("WORKLOAD_NAME"), id.to_string());
        let new_filter = filter_init(envoy_properties);
        PluginWrapper {
            filter: new_filter,
            loaded_function: loaded_function,
            id: id,
            stored_rpc: None,
            neighbor: None,
        }
    }

    pub fn execute(&self, input: &Rpc) -> Option<Rpc> {
        (self.loaded_function)(&self.filter, input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_plugin_creation() {
        let mut cargo_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        cargo_dir.push("../target/debug/libfilter_lib");
        let library_str = cargo_dir.to_str().unwrap();
        let plugin = PluginWrapper::new(library_str, 0);
        let rpc = &Rpc::new_rpc(55);
        let rpc_data = plugin.execute(rpc).unwrap().data;
        assert!(rpc_data == 55);
    }

    #[test]
    fn test_chained_plugins() {
        let mut cargo_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        cargo_dir.push("../target/debug/libfilter_lib");
        let library_str = cargo_dir.to_str().unwrap();
        let plugin1 = PluginWrapper::new(library_str, 0);
        let plugin2 = PluginWrapper::new(library_str, 1);
        let plugin3 = PluginWrapper::new(library_str, 2);
        let plugin4 = PluginWrapper::new(library_str, 3);
        assert!(
            5 == plugin4
                .execute(
                    &plugin3
                        .execute(
                            &plugin2
                                .execute(&plugin1.execute(&Rpc::new_rpc(5)).unwrap())
                                .unwrap()
                        )
                        .unwrap()
                )
                .unwrap()
                .data
        );
    }
}