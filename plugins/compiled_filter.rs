mod rpc;
use std::collections::HashMap;
use std::fs;
//mod graph_utils;

pub type CodeletType = fn(&Filter, &rpc::Rpc) -> Option<rpc::Rpc>;


// user defined functions:
// init_func: new
// exec_func: execute
// struct_name: Count
// id: count

#[derive(Clone, Copy, Debug)]
pub struct Count {
    counter: u32
}

impl Count {
    fn new() -> Count {
        Count { counter: 0 }
    }
    fn execute(mut self) -> u32 {
        self.counter = self.counter + 1;
        self.counter
    }
}



// This represents a piece of state of the filter
// it either contains a user defined function, or some sort of 
// other persistent state
pub struct State {
    pub type_of_state: Option<String>,
    pub string_data: Option<String>,
    pub udf_count: Option<Count>,
}

impl State {
    pub fn new() -> State {
        State { 
            type_of_state: None,
            string_data: None,
            udf_count:  None ,
        }
    } 

    pub fn new_with_str(str_data: String) -> State {
        State { 
            type_of_state: Some(String::from("String")),
            string_data: Some(str_data),
            udf_count:  None ,
        }
    } 
}

pub struct Filter {
    pub filter_state: HashMap<String, State>,
}

impl Filter {
    #[no_mangle]
    pub fn new() -> Filter {
        Filter { 
	    filter_state: HashMap::new(),
	}
    }

    #[no_mangle]
    pub fn new_with_envoy_properties(string_data: HashMap<String, String>) -> Filter {
         let mut hash = HashMap::new();
         for key in string_data.keys() {
             hash.insert(key.clone(), State::new_with_str(string_data[key].clone()));
         }
         let new_filter = Filter { 
	    filter_state: hash,
	 };
         return new_filter;
    }

    #[no_mangle]
    pub fn execute(&mut self, x: &rpc::Rpc) -> Option<rpc::Rpc> {
        // 0. Who am I?
        let my_node = self.filter_state["WORKLOAD_NAME"].string_data.clone().unwrap();

        // 1. Do I need to put any udf variables/objects in?
        
        if !self.filter_state.contains_key("count") {
            let mut new_state = State::new();
            new_state.type_of_state = Some(String::from("count"));
            new_state.udf_count = Some(Count::new());
            self.filter_state.insert(String::from("count"), new_state);
        }
        

        // 2. TODO: Find the node attributes to be collected

        // 3.  Make a subgraph representing the query, check isomorphism compared to the
        //     observed trace, and do return calls based on that info
        if my_node == String::from("0") {
            // we need to create the graph given by the query
            let vertices = vec![ "n", "m",   ];
            let edges = vec![  ( "n", "m",  ),  ];
            let mut ids_to_properties: HashMap<&str, Vec<&str>> = HashMap::new();
            
            ids_to_properties.insert("a", vec![  "node",  "metadata",  "WORKLOAD_NAME",  ]);
            


            /*
            let target_graph = generate_target_graph(vertices, edges, ids_to_properties);
            let trace_graph = generate_trace_graph_from_headers(x.path);
            let mapping = get_sub_graph_mapping(trace_graph, target_graph); 
            if mapping.len() > 0 {
                // In the non-simulator version, we will send the result to storage.  Given this is 
                // a simulation, we will write it to a file.
                
                let obj = self.filter_state["count"].udf_count.unwrap().clone();
                let value = obj.execute().to_string();
                fs::write("result.txt", value).expect("Unable to write file"); 
                
       
            }
            */
        }

        // 4.  Store udf results
        
        let obj = self.filter_state["count"].udf_count.unwrap().clone();
        obj.execute();
        


        // 5.  Pass the rpc on
        Some(rpc::Rpc{ 
            data: x.data, uid: x.uid , path: x.path.clone()
             }   ) 
    }

}