use indexmap::map::IndexMap;
use log4rs::{
    append::{
        console::{ConsoleAppender, Target},
        file::FileAppender,
    },
    config::{Appender, Config, Root},
    encode::pattern::PatternEncoder,
    filter::threshold::ThresholdFilter,
};
use petgraph::graph::{Graph, NodeIndex};
use petgraph::Incoming;
use rpc_lib::rpc::Rpc;
use utils::graph::graph_utils;
use utils::graph::iso::find_mapping_shamir_centralized;
use utils::graph::serde::FerriedData;

extern crate serde_json;

pub type CodeletType = fn(&Filter, &Rpc) -> Option<Rpc>;
fn log_setup() {
    // Build a stderr logger.
    let stderr = ConsoleAppender::builder()
        .encoder(Box::new(PatternEncoder::new("{h({l})}: {m}\n")))
        .target(Target::Stderr)
        .build();
    // Logging to log file.
    let logfile = FileAppender::builder()
        // Pattern: https://docs.rs/log4rs/*/log4rs/encode/pattern/index.html
        .encoder(Box::new(PatternEncoder::new("{l}: {m}\n")))
        .append(false)
        .build("sim.log")
        .unwrap();
    // Log Trace level output to file where trace is the default level
    // and the programmatically specified level to stderr.
    let config = Config::builder()
        .appender(Appender::builder().build("logfile", Box::new(logfile)))
        .appender(
            Appender::builder()
                .filter(Box::new(ThresholdFilter::new(log::LevelFilter::Info)))
                .build("stderr", Box::new(stderr)),
        )
        .build(
            Root::builder()
                .appender("logfile")
                .appender("stderr")
                .build(log::LevelFilter::Trace),
        )
        .unwrap();
    // Use this to change log levels at runtime.
    // This means you can change the default log level to trace
    // if you are trying to debug an issue and need more logs on then turn it off
    // once you are done.
    let _handle = log4rs::init_config(config);
}

fn put_ferried_data_in_hdrs(fd: &mut FerriedData, hdr: &mut IndexMap<String, String>) {
    match serde_json::to_string(fd) {
        Ok(stored_data_string) => {
            hdr.insert("ferried_data".to_string(), stored_data_string);
        }
        Err(e) => {
            log::error!(
                "ERROR:  could not translate stored data to json string: {0}\n",
                e
            );
        }
    }
}

// user defined functions:
// udf_type: Scalar
// leaf_func: leaf_height
// mid_func: mid_height
// id: height

fn leaf_height(_graph: &Graph<(String, IndexMap<String, String>), ()>) -> u32 {
    return 0;
}

// TODO:  must children's responses always be in string form?  can we generalize?
fn mid_height(
    _graph: &Graph<(String, IndexMap<String, String>), ()>,
    children_responses: Vec<String>,
) -> u32 {
    let mut max = 0;
    for response in children_responses {
        let response_as_u32 = response.parse::<u32>();
        match response_as_u32 {
            Ok(num) => {
                if num > max {
                    max = num;
                }
            }
            Err(e) => {
                print!("error: {0}\n", e);
            }
        }
    }
    return max + 1;
}

pub fn create_target_graph() -> Graph<
    (
        std::string::String,
        IndexMap<std::string::String, std::string::String>,
    ),
    (),
> {
    let vertices = vec!["a".to_string(), "b".to_string(), "c".to_string()];
    let edges = vec![
        ("a".to_string(), "b".to_string()),
        ("b".to_string(), "c".to_string()),
    ];
    let mut ids_to_properties: IndexMap<String, IndexMap<String, String>> = IndexMap::new();
    ids_to_properties.insert("a".to_string(), IndexMap::new());
    ids_to_properties.insert("b".to_string(), IndexMap::new());
    ids_to_properties.insert("c".to_string(), IndexMap::new());
    return graph_utils::generate_target_graph(vertices, edges, ids_to_properties);
}

pub fn collect_envoy_properties(_filter: &Filter, _fd: &mut FerriedData) {}

pub fn execute_udfs_and_check_trace_lvl_prop(filter: &Filter, fd: &mut FerriedData) -> bool {
    let my_height_value;
    let child_iterator = fd.trace_graph.neighbors_directed(
        graph_utils::get_node_with_id(&fd.trace_graph, filter.whoami.as_ref().unwrap().clone())
            .unwrap(),
        petgraph::Outgoing,
    );
    let mut child_values = Vec::new();
    for child in child_iterator {
        child_values.push(fd.trace_graph.node_weight(child).unwrap().1["height"].clone());
    }
    if child_values.len() == 0 {
        my_height_value = leaf_height(&fd.trace_graph).to_string();
    } else {
        my_height_value = mid_height(&fd.trace_graph, child_values).to_string();
    }

    let node =
        graph_utils::get_node_with_id(&fd.trace_graph, filter.whoami.as_ref().unwrap().to_string())
            .unwrap();
    // if we already have the property, don't add it
    if !(fd
        .trace_graph
        .node_weight(node)
        .unwrap()
        .1
        .contains_key("height")
        && fd.trace_graph.node_weight(node).unwrap().1["height"] == my_height_value)
    {
        fd.trace_graph
            .node_weight_mut(node)
            .unwrap()
            .1
            .insert("height".to_string(), my_height_value);
    }

    return true;
}

pub fn get_value_for_storage(
    target_graph: &Graph<
        (
            std::string::String,
            IndexMap<std::string::String, std::string::String>,
        ),
        (),
    >,
    mapping: &Vec<(NodeIndex, NodeIndex)>,
    fd: &FerriedData,
) -> Option<String> {
    let value: String;
    let node_ptr = graph_utils::get_node_with_id(target_graph, "a".to_string());
    if node_ptr.is_none() {
        log::warn!("Node a not found");
        return None;
    }
    let mut trace_node_index = None;
    for map in mapping {
        if target_graph.node_weight(map.0).unwrap().0 == "a" {
            trace_node_index = Some(map.1);
            break;
        }
    }
    if trace_node_index == None
        || !&fd
            .trace_graph
            .node_weight(trace_node_index.unwrap())
            .unwrap()
            .1
            .contains_key("height")
    {
        // we have not yet collected the return property or have a mapping error
        return None;
    }
    let ret_height = &fd
        .trace_graph
        .node_weight(trace_node_index.unwrap())
        .unwrap()
        .1["height"];

    value = ret_height.to_string();

    return Some(value);
}

#[derive(Clone, Debug)]
pub struct Filter {
    pub whoami: Option<String>,
    pub target_graph: Option<Graph<(String, IndexMap<String, String>), ()>>,
    pub filter_state: IndexMap<String, String>,
    pub envoy_shared_data: IndexMap<String, String>, // trace ID to stored ferried data as string
    pub collected_properties: Vec<String>,           //properties to collect
}

impl Filter {
    #[no_mangle]
    pub fn new() -> *mut Filter {
        log_setup();
        Box::into_raw(Box::new(Filter {
            whoami: None,
            target_graph: None,
            filter_state: IndexMap::new(),
            envoy_shared_data: IndexMap::<String, String>::new(),
            collected_properties: vec!["height".to_string()],
        }))
    }

    #[no_mangle]
    pub fn new_with_envoy_properties(string_data: IndexMap<String, String>) -> *mut Filter {
        log_setup();
        Box::into_raw(Box::new(Filter {
            whoami: None,
            target_graph: None,
            filter_state: string_data,
            envoy_shared_data: IndexMap::new(),
            collected_properties: vec!["height".to_string()],
        }))
    }

    pub fn init_filter(&mut self) {
        if self.whoami.is_none() {
            self.set_whoami();
            assert!(self.whoami.is_some());
        }
        if self.target_graph.is_none() {
            self.target_graph = Some(create_target_graph());
        }
        assert!(self.whoami.is_some());
    }

    pub fn set_whoami(&mut self) {
        if !self
            .filter_state
            .contains_key("node.metadata.WORKLOAD_NAME")
        {
            log::warn!("filter was initialized without envoy properties and thus cannot function");
            return;
        }
        let my_node = self.filter_state["node.metadata.WORKLOAD_NAME"].clone();
        self.whoami = Some(my_node);
        assert!(self.whoami.is_some());
    }

    pub fn store_headers(&mut self, uid_64: u64, headers: IndexMap<String, String>) {
        // If you don't have data, nothing to store
        if !headers.contains_key("ferried_data") {
            log::warn!("no ferried data\n");
            return;
        }
        let uid = uid_64.to_string();
        // If there is no data stored, you needn't merge - just throw it in
        if !self.envoy_shared_data.contains_key(&uid) {
            self.envoy_shared_data
                .insert(uid.clone(), headers["ferried_data"].clone());
        }

        // Else, we merge in 2 parts, for each of the struct values
        let mut data: FerriedData;
        let mut stored_data: FerriedData;

        match serde_json::from_str(&headers["ferried_data"]) {
            Ok(d) => {
                data = d;
            }
            Err(e) => {
                log::error!("could not parse envoy shared data: {0}\n", e);
                return;
            }
        }
        match serde_json::from_str(&self.envoy_shared_data[&uid]) {
            Ok(d) => {
                stored_data = d;
            }
            Err(e) => {
                log::error!("could not parse envoy shared data: {0}\n", e);
                return;
            }
        }

        // 2. Merge the graphs by simply adding it - later, when we merge, we will
        //    make a root

        // add node
        for node in data.trace_graph.node_indices() {
            stored_data
                .trace_graph
                .add_node(data.trace_graph.node_weight(node).unwrap().clone());
        }
        // add edges
        for edge in data.trace_graph.edge_indices() {
            match data.trace_graph.edge_endpoints(edge) {
                Some((edge0, edge1)) => {
                    let edge0_weight = &data.trace_graph.node_weight(edge0).unwrap().0;
                    let edge1_weight = &data.trace_graph.node_weight(edge1).unwrap().0;
                    let edge0_in_stored_graph = graph_utils::get_node_with_id(
                        &stored_data.trace_graph,
                        edge0_weight.to_string(),
                    )
                    .unwrap();
                    let edge1_in_stored_graph = graph_utils::get_node_with_id(
                        &stored_data.trace_graph,
                        edge1_weight.to_string(),
                    )
                    .unwrap();
                    stored_data.trace_graph.add_edge(
                        edge0_in_stored_graph,
                        edge1_in_stored_graph,
                        (),
                    );
                }
                None => {
                    log::error!("no edge endpoints found \n");
                    return;
                }
            }
        }

        // 3. merge unassigned properties
        //    these are properties we have collected but are not yet in the graph
        stored_data
            .unassigned_properties
            .append(&mut data.unassigned_properties);
        stored_data.unassigned_properties.sort_unstable();
        stored_data.unassigned_properties.dedup();
        stored_data.assign_properties();

        match serde_json::to_string(&stored_data) {
            Ok(stored_data_string) => {
                self.envoy_shared_data.insert(uid, stored_data_string);
            }
            Err(e) => {
                log::error!("could not translate stored data to json string: {0}\n", e);
            }
        }
    }

    pub fn merge_headers(
        &mut self,
        uid: u64,
        mut new_rpc_headers: IndexMap<String, String>,
    ) -> IndexMap<String, String> {
        let uid_str = uid.to_string();
        let mut my_indexmap = IndexMap::new();
        my_indexmap.insert(
            "node.metadata.WORKLOAD_NAME".to_string(),
            self.whoami.as_ref().unwrap().clone(),
        );

        if self.envoy_shared_data.contains_key(&uid_str) {
            match serde_json::from_str(&self.envoy_shared_data[&uid_str]) {
                Ok(d) => {
                    // 1. TODO:  if needed, do things to set S
                    // 2. If response, add yourself as root
                    if new_rpc_headers["direction"] == "response" {
                        let mut data: FerriedData = d;
                        let mut previous_roots = Vec::new();
                        for node in data.trace_graph.node_indices() {
                            if data.trace_graph.neighbors_directed(node, Incoming).count() == 0 {
                                previous_roots.push(node);
                            }
                        }
                        let me = data
                            .trace_graph
                            .add_node((self.whoami.as_ref().unwrap().to_string(), my_indexmap));

                        for previous_root in previous_roots {
                            data.trace_graph.add_edge(me, previous_root, ());
                        }
                        data.assign_properties();

                        // Finally, put all the data back in the headers
                        put_ferried_data_in_hdrs(&mut data, &mut new_rpc_headers);
                    }
                }
                Err(e) => {
                    log::error!("could not parse envoy shared data: {0}\n", e);
                }
            }
        } else {
            let mut new_ferried_data = FerriedData::default();
            new_ferried_data
                .trace_graph
                .add_node((self.whoami.as_ref().unwrap().to_string(), my_indexmap));
            put_ferried_data_in_hdrs(&mut new_ferried_data, &mut new_rpc_headers);
        }
        return new_rpc_headers;
    }

    pub fn on_incoming_requests(&mut self, mut x: Rpc) -> Vec<Rpc> {
        // Fetch ferried data
        let mut ferried_data: FerriedData;
        if !x.headers.contains_key("ferried_data") {
            ferried_data = FerriedData::default();
        } else {
            match serde_json::from_str(&x.headers["ferried_data"]) {
                Ok(fd) => {
                    ferried_data = fd;
                }
                Err(e) => {
                    log::error!("could not translate stored data to json string: {0}\n", e);
                    return vec![x];
                }
            }
        }

        // Insert properties to collect
        collect_envoy_properties(self, &mut ferried_data);

        // Return ferried data to x, and store headers
        put_ferried_data_in_hdrs(&mut ferried_data, &mut x.headers);
        self.store_headers(x.uid, x.headers.clone());
        return vec![x];
    }

    pub fn on_outgoing_responses(&mut self, mut x: Rpc) -> Vec<Rpc> {
        // 0. Look up stored baggage, and merge it
        x.headers = self.merge_headers(x.uid, x.headers);

        // at most, we return two rpcs:  one to continue on and one to storage
        let mut original_rpc = x.clone();
        let mut storage_rpc: Rpc;

        // 1. retrieve our ferried data, containing the newly merged
        //    baggage
        let mut ferried_data: FerriedData;
        if !original_rpc.headers.contains_key("ferried_data") {
            ferried_data = FerriedData::default();
        } else {
            match serde_json::from_str(&mut original_rpc.headers["ferried_data"]) {
                Ok(fd) => {
                    ferried_data = fd;
                }
                Err(e) => {
                    log::error!("could not parse ferried data: {0}\n", e);
                    return vec![original_rpc];
                }
            }
        }

        let root_id = "productpage-v1";
        let trace_prop_sat = execute_udfs_and_check_trace_lvl_prop(self, &mut ferried_data);
        // 3. perform isomorphism and possibly return if root node
        if trace_prop_sat && self.whoami.as_ref().unwrap() == root_id {
            let mapping = find_mapping_shamir_centralized(
                &ferried_data.trace_graph,
                self.target_graph.as_ref().unwrap(),
            );
            if mapping.is_some() {
                let m = mapping.unwrap();
                let value =
                    get_value_for_storage(self.target_graph.as_ref().unwrap(), &m, &ferried_data);
                if value.is_none() {
                    put_ferried_data_in_hdrs(&mut ferried_data, &mut original_rpc.headers);
                    return vec![original_rpc];
                }
                // Now you have the return value, so
                // 3a. Make a storage rpc
                storage_rpc = Rpc::new_with_src(&value.unwrap(), self.whoami.as_ref().unwrap());
                storage_rpc
                    .headers
                    .insert("dest".to_string(), "storage".to_string());
                storage_rpc
                    .headers
                    .insert("direction".to_string(), "request".to_string());
                storage_rpc
                    .headers
                    .insert("src".to_string(), self.whoami.clone().unwrap());

                // 3b. Put baggage into regular rpc
                put_ferried_data_in_hdrs(&mut ferried_data, &mut original_rpc.headers);
                return vec![original_rpc, storage_rpc];
            }
        }
        put_ferried_data_in_hdrs(&mut ferried_data, &mut original_rpc.headers);
        return vec![original_rpc];
    }

    pub fn on_outgoing_requests(&mut self, mut x: Rpc) -> Vec<Rpc> {
        x.headers = self.merge_headers(x.uid, x.headers);
        return vec![x];
    }

    pub fn on_incoming_responses(&mut self, x: Rpc) -> Vec<Rpc> {
        self.store_headers(x.uid, x.headers.clone());
        return vec![x];
    }

    #[no_mangle]
    pub fn execute(&mut self, x: &Rpc) -> Vec<Rpc> {
        self.init_filter();
        assert!(self.whoami.is_some());
        match x.headers["direction"].as_str() {
            "request" => match x.headers["location"].as_str() {
                "ingress" => {
                    return self.on_incoming_requests(x.clone());
                }
                "egress" => {
                    return self.on_outgoing_requests(x.clone());
                }
                _ => {
                    panic!("Filter got an rpc with no location\n");
                }
            },
            "response" => match x.headers["location"].as_str() {
                "ingress" => {
                    return self.on_incoming_responses(x.clone());
                }
                "egress" => {
                    return self.on_outgoing_responses(x.clone());
                }
                _ => {
                    panic!("Filter got an rpc with no location\n");
                }
            },
            _ => {
                panic!("Filter got an rpc with no direction\n");
            }
        }
    }
}
