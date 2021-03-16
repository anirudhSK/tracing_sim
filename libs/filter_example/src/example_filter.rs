use rpc_lib::rpc::Rpc;
use std::collections::HashMap;
use petgraph::graph::NodeIndex;
use petgraph::Graph;
use petgraph::Outgoing;
use graph_utils::utils;
use graph_utils::iso::find_mapping_shamir_centralized;

pub type CodeletType = fn(&Filter, &Rpc) -> Option<Rpc>;

// user defined functions:
// udf_type: Scalar
// leaf_func: leaf_height
// mid_func: mid_height
// id: height

fn leaf_height(_graph: Graph<(String, HashMap<String, String>), String>) -> u32 {
    return 0;
}

// TODO:  must children's responses always be in string form?  can we generalize?
fn mid_height(_graph: Graph<(String, HashMap<String, String>), String>, children_responses: Vec<String>) -> u32 {
    let mut max = 0;
    for response in children_responses {
        let response_as_u32 = response.parse::<u32>();
            match response_as_u32 {
                Ok(num) => { if num > max { max = num; } }
                Err(e) => { print!("error: {0}\n", e); }
            }
    }
    return max + 1;
}



// This represents a piece of state of the filter
// it contains information that we get from calling getValue in the real filters
// TODO:  since we no longer use these to define UDFs, we have no need for it
// to be anything other than a string.  We should simplify it to simply have
// an Option<String> for every possible envoy value

#[derive(Clone, Debug)]
pub struct State {
    pub type_of_state: Option<String>,
    pub string_data: Option<String>,
}

impl State {
    pub fn new() -> State {
        State {
            type_of_state: None,
            string_data: None,
        }
    }

    pub fn new_with_str(str_data: String) -> State {
        State {
            type_of_state: Some(String::from("String")),
            string_data: Some(str_data),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Filter {
    pub whoami: Option<String>,
    pub target_graph: Option<Graph<(String, HashMap<String, String>), String>>,
    pub filter_state: HashMap<String, State>,
    pub envoy_shared_data: HashMap<String, String>,
    pub collected_properties: Vec<String>, //properties to collect
}

impl Filter {
    #[no_mangle]
    pub fn new() -> *mut Filter {
         Box::into_raw(Box::new(Filter {
            whoami: None,
            target_graph: None,
            filter_state: HashMap::new(),
            envoy_shared_data: HashMap::<String, String>::new(),
            collected_properties: vec!( "service_name".to_string(), "height".to_string(),  ),
         }))
    }

    #[no_mangle]
    pub fn new_with_envoy_properties(string_data: HashMap<String, String>) -> *mut Filter {
        let mut hash = HashMap::new();
        for key in string_data.keys() {
            hash.insert(key.clone(), State::new_with_str(string_data[key].clone()));
        }
        Box::into_raw(Box::new(Filter {
                                   whoami: None,
                                   target_graph: None,
                                   filter_state: hash,
                                   envoy_shared_data: HashMap::new(),
                                   collected_properties: vec!("service_name".to_string(), "height".to_string(),  ),
                               }))
     }

    pub fn init_filter(&mut self) {
        if self.whoami.is_none() { self.set_whoami(); assert!(self.whoami.is_some()); }
        if self.target_graph.is_none() { self.create_target_graph(); } 
        assert!(self.whoami.is_some());
    }

    pub fn set_whoami(&mut self) {
        let my_node_wrapped = self
            .filter_state
            .get("node.metadata.WORKLOAD_NAME");
        if my_node_wrapped.is_none() {
            print!("WARNING: filter was initialized without envoy properties and thus cannot function");
            return;
        }
        let my_node = my_node_wrapped
            .unwrap()
            .string_data
            .clone()
            .unwrap();
        self.whoami = Some(my_node);
        assert!(self.whoami.is_some());
    }

    pub fn store_headers(&mut self, x: Rpc) {
        // store path as well as properties
        let prop_str = format!("{uid}_properties_path", uid=x.uid);
        if x.headers.contains_key("properties_path") {
            if self.envoy_shared_data.contains_key(&prop_str) {
                self.envoy_shared_data.get_mut(&prop_str).unwrap().push_str(",");
                self.envoy_shared_data.get_mut(&prop_str).unwrap().push_str(&x.headers["properties_path"]);
                self.envoy_shared_data.get_mut(&prop_str).unwrap().push_str(";");
                self.envoy_shared_data.get_mut(&prop_str).unwrap().push_str(self.whoami.as_ref().unwrap());
            }
            else {
                // add yourself
                let mut cur_path = x.headers["properties_path"].clone();
                cur_path.push_str(";");
                cur_path.push_str(self.whoami.as_ref().unwrap());
                self.envoy_shared_data.insert(prop_str, cur_path);
            }
        }
        for key in &self.collected_properties {
            let prop_str = format!("{uid}_properties_{key}", uid=x.uid, key=key);
            let prop_key = format!("properties_{key}", key=key);
            if x.headers.contains_key(&prop_key) {
                if self.envoy_shared_data.contains_key(&prop_str) { // concatenate with comma if not duplicate
                    // make lists of properties
                    let properties_unified = self.envoy_shared_data[&prop_str].clone();
                    let mut properties : Vec<String> = properties_unified.split(",").map(|s| s.to_string()).collect();
                    let cur_properties_unified = x.headers[&prop_key].clone();
                    let mut cur_properties : Vec<String> = cur_properties_unified.split(",").map(|s| s.to_string()).collect();

                    // merge lists and remove duplicates
                    properties.append(&mut cur_properties);
                    properties.sort_unstable(); // must sort for dedup to work
                    properties.dedup();

                    // store result
                    let property_string_to_store = properties.join(",");
                    self.envoy_shared_data.insert(prop_str.clone(), property_string_to_store.clone());
                } else {
                    self.envoy_shared_data.insert(prop_str.clone(), x.headers[&prop_key].clone());
                }
            }
        }
    }

    pub fn merge_headers(&mut self, uid: u64, mut new_rpc_headers: HashMap<String, String>) -> HashMap<String, String> {
        // if we are a response, we should do path bookkeeping
        if new_rpc_headers["direction"] == "response" {
            let prop_str = format!("{uid}_properties_path", uid=uid);
            if self.envoy_shared_data.contains_key(&prop_str) {
                new_rpc_headers.insert("properties_path".to_string(), self.envoy_shared_data[&prop_str].clone());
            }
            else { new_rpc_headers.insert("properties_path".to_string(), self.whoami.as_ref().unwrap().clone()); }
        }

        // all other properties
        for key in &self.collected_properties {
            let prop_str = format!("{uid}_properties_{key}", uid=uid, key=key);
            let prop_key = format!("properties_{key}", key=key);
            if self.envoy_shared_data.contains_key(&prop_str) {
                new_rpc_headers.insert(prop_key, self.envoy_shared_data[&prop_str].clone());
            } else {
                if new_rpc_headers["direction"] != "request" { 
                    println!("WARNING: could not find value for {0}", prop_str); 
                }
            }
        }
        return new_rpc_headers;
    }

    pub fn create_target_graph(&mut self) {
         let vertices = vec!(  "a".to_string(), "b".to_string(), "c".to_string(),  );
         let edges = vec!(   ("a".to_string(), "b".to_string() ),   ("b".to_string(), "c".to_string() ),   );
         let mut ids_to_properties: HashMap<String, HashMap<String, String>> = HashMap::new();
         ids_to_properties.insert("a".to_string(), HashMap::new());
         ids_to_properties.insert("b".to_string(), HashMap::new());
         ids_to_properties.insert("c".to_string(), HashMap::new());
         let mut b_hashmap = ids_to_properties.get_mut("b").unwrap();
         b_hashmap.insert("service_name".to_string(), "reviews-v1".to_string());
         self.target_graph = Some(utils::generate_target_graph(vertices, edges, ids_to_properties));
 
    }

    pub fn create_trace_graph(&mut self, mut mod_rpc: Rpc) -> Graph<(String, HashMap<String, String>), String> {
        let trace;
        let mut path = mod_rpc.headers["properties_path"].clone();
        let mut properties = Vec::new();
        for header in mod_rpc.headers.keys() {
            if header.contains("properties_") && header != "properties_path" {
                properties.push(mod_rpc.headers[header].clone().replace("properties_", ""));
            }
        }
        let prop_to_trace = properties.join(",");
        trace = utils::generate_trace_graph_from_headers(path, prop_to_trace);
        return trace;
    }

    pub fn on_incoming_requests(&mut self, mut x: Rpc) -> Vec<Rpc> {
        let mut prop_str;
        prop_str = format!("{whoami}.{property}=={value}",
                                                      whoami=&self.whoami.as_ref().unwrap(),
                                                      property="service_name",
                                                      value=self.filter_state["node.metadata.WORKLOAD_NAME"].string_data.as_ref().unwrap().to_string());
                                             
        if x.headers.contains_key("properties_service_name") {
            if !x.headers["properties_service_name"].contains(&prop_str) { // don't add our properties if they have already been added
                x.headers.get_mut(&"properties_service_name".to_string()).unwrap().push_str(",");
                x.headers.get_mut(&"properties_service_name".to_string()).unwrap().push_str(&prop_str);
            }
        }
        else {
            x.headers.insert("properties_service_name".to_string(), prop_str);
        }
         
        self.store_headers(x.clone());
        return vec!(x);
    }

    pub fn on_outgoing_responses(&mut self, mut x: Rpc) -> Vec<Rpc> {
        x.headers = self.merge_headers(x.uid, x.headers);

        assert!(x.headers.contains_key("properties_path"));

        // at most, we return two rpcs:  one to continue on and one to storage
        let mut original_rpc = x.clone();
        let mut storage_rpc : Option<Rpc> = None;

        // calculate UDFs and store result
        let my_height_value;
    if x.headers.contains_key("properties_path") {
            // TODO:  only create trace graph once and then add to it
            let graph = self.create_trace_graph(x.clone());
            let child_iterator = graph.neighbors_directed(
                utils::get_node_with_id(&graph, self.whoami.as_ref().unwrap().clone()).unwrap(),
                petgraph::Outgoing);
            let mut child_values = Vec::new();
            for child in child_iterator {
                child_values.push(graph.node_weight(child).unwrap().1["height"].clone());
            }
            if child_values.len() == 0 {
                my_height_value = leaf_height(graph);
            } else {
                my_height_value = mid_height(graph, child_values);
            }
         }
         else {
             print!("WARNING: no path header");
             return vec!(x);
         }

         let height_str = format!("{whoami}.{udf_id}=={value}",
                                                      whoami=&self.whoami.as_ref().unwrap(),
                                                      udf_id="properties_height",
                                                      value=my_height_value);
        if x.headers.contains_key("properties_height") {
            if !x.headers["properties_height"].contains(&height_str) { // don't add a udf property twice
                x.headers.get_mut(&"properties_height".to_string()).unwrap().push_str(",");
                x.headers.get_mut(&"properties_height".to_string()).unwrap().push_str(&height_str);
            }
        }
        else {
            x.headers.insert("properties_height".to_string(), height_str);
        }

         

        let mut trace_graph = self.create_trace_graph(x.clone());
        let mapping = find_mapping_shamir_centralized(
            &trace_graph,
            self.target_graph.as_ref().unwrap(),
        );
        if !mapping.is_none() {
            let m = mapping.unwrap();
            let mut value = "0".to_string(); // dummy value
            // TODO: do return stuff
            let node_ptr = utils::get_node_with_id(&self.target_graph.as_ref().unwrap(), "a".to_string());
               if node_ptr.is_none() {
                   print!("WARNING Node a not found");
                   return vec!(x);
               }
               let mut trace_node_index = None;
               for map in m {
                   if self.target_graph.as_ref().unwrap().node_weight(map.0).unwrap().0 == "a" {
                       trace_node_index = Some(map.1);
                       break;
                   }
               }
               if trace_node_index == None || !&trace_graph.node_weight(trace_node_index.unwrap()).unwrap().1.contains_key("height") {
                   // we have not yet collected the return property or have a mapping error
                   return vec!(x);
               }
               let mut ret_height = &trace_graph.node_weight(trace_node_index.unwrap()).unwrap().1[ "height" ];

               value = ret_height.to_string();
 
            let mut result_rpc = Rpc::new_with_src(&value, self.whoami.as_ref().unwrap());
            let mut dest = "storage".to_string();
            result_rpc
                .headers
                .insert("dest".to_string(), dest);
            result_rpc
                .headers
                .insert("direction".to_string(), "request".to_string());
            result_rpc.headers.insert("src".to_string(), self.whoami.clone().unwrap());
            storage_rpc = Some(result_rpc);
            return vec!(x, storage_rpc.unwrap());
       }
       return vec!(x);

    }

    pub fn on_outgoing_requests(&mut self, mut x: Rpc) -> Vec<Rpc>{
        x.headers = self.merge_headers(x.uid, x.headers);
        return vec!(x);
    }

    pub fn on_incoming_responses(&mut self, mut x: Rpc) -> Vec<Rpc> {
        self.store_headers(x.clone());
        return vec!(x);
    }


    #[no_mangle]
    pub fn execute(&mut self, x: &Rpc) -> Vec<Rpc> {
        self.init_filter();
        assert!(self.whoami.is_some());
        match x.headers["direction"].as_str() {
            "request" => {
                 match x.headers["location"].as_str() {
                 "ingress" => { return self.on_incoming_requests(x.clone()); }
                 "egress" => { return self.on_outgoing_requests(x.clone()); }
                 _ => { panic!("Filter got an rpc with no location\n"); }
                 }
             }
             "response" => {
                 match x.headers["location"].as_str() {
                 "ingress" => { return self.on_incoming_responses(x.clone()); }
                 "egress" => { return self.on_outgoing_responses(x.clone()); }
                 _ => { panic!("Filter got an rpc with no location\n"); }
                 }
             }
             _ => { panic!("Filter got an rpc with no direction\n"); }
        }
    }

}
