/* This file contains functions relating to creating and comparing trace and target (user-given) graphs */

use petgraph::algo::{dijkstra, toposort};
use petgraph::graph::{Graph, NodeIndex};
use petgraph::visit::DfsPostOrder;
use petgraph::Incoming;
use regex::Regex;
use std::collections::HashMap;

/* This function creates a petgraph graph representing the query given by the user.
 * For example, if the cql query were MATCH n -> m, e WHERE ... the input to this function
 * would be vertices = [n, m], edges = [(n,m)].
 *
 * Arguments:
 * @vertices:  the vertices of the graph to construct
 * @edges:  the edges of the graph to construct
 *
 * Return Value:
 * @graph: the constructed graph reprsenting the inputs
 */

pub fn generate_target_graph(
    vertices: Vec<String>,
    edges: Vec<(String, String)>,
    ids_to_properties: HashMap<String, HashMap<String, String>>,
) -> Graph<(String, HashMap<String, String>), String> {
    let mut graph = Graph::new();

    // In order to make edges, we have to know the handles of the nodes, and you
    // get the handles of the nodes by adding them to the graph

    let mut nodes_to_node_handles: HashMap<String, NodeIndex> = HashMap::new();
    for node in vertices {
        if ids_to_properties.contains_key(&node) {
            nodes_to_node_handles.insert(
                node.clone(),
                graph.add_node((node.clone(), ids_to_properties[&node].clone())),
            );
        } else {
            nodes_to_node_handles
                .insert(node.clone(), graph.add_node((node.clone(), HashMap::new())));
        }
    }

    // Make edges with handles instead of the vertex names
    let mut edge_handles = Vec::new();
    for edge in edges {
        let node0 = nodes_to_node_handles[&edge.0];
        let node1 = nodes_to_node_handles[&edge.1];
        let new_edge = (node0, node1);
        edge_handles.push(new_edge);
    }
    graph.extend_with_edges(edge_handles);

    graph
}

/*  This function creates a petgraph graph representing a single trace.
 *  The trace is represented in paths_header as a string.  Comma separated strings represent
 *  straight paths, and those straight paths share nodes that represent a node with multiple children
 *  Each node is separated by a semicolon
 *  Thus "2;1;0,3;1" is a graph that looks like 0 -> 1 -> 2 , where 1 also has another child, 3,
 *  and 0 is root.
 *
 *  Arguments:
 *  @paths_header:  the string version of the trace, generated by the tracing simulator
 *  @properties_header: the string containing the properties of the nodes in the path
 *  It is of the form node_name.property=="xyz" and is comma separated
 *  For example, "a.response.total_size==4,b.response.total_size==5"
 *
 *  Return Value:
 *  @graph:  A petgraph graph representation of the same trace
 */
pub fn generate_trace_graph_from_headers(
    paths_header: String,
    properties_header: String,
) -> Graph<(String, HashMap<String, String>), String> {
    let mut graph: Graph<(String, HashMap<String, String>), String> = Graph::new();
    if paths_header.is_empty() {
        return graph;
    }
    let name = "node.metadata.WORKLOAD_NAME";
    let mut node_handles: HashMap<String, NodeIndex> = HashMap::new();
    let straight_paths = paths_header.split(",");
    for straight_path in straight_paths {
        let mut node_iterator = straight_path.split(";");
        let mut node_str = node_iterator.next().unwrap();
        if !node_handles.contains_key(node_str) {
            let node_hashmap: HashMap<String, String> = [(name.to_string(), node_str.to_string())]
                .iter()
                .cloned()
                .collect();
            node_handles.insert(
                node_str.to_string(),
                graph.add_node((node_str.to_string(), node_hashmap)),
            );
        }
        while let Some(new_node_str) = node_iterator.next() {
            if !node_handles.contains_key(new_node_str) {
                let new_node_hashmap: HashMap<String, String> =
                    [(name.to_string(), new_node_str.to_string())]
                        .iter()
                        .cloned()
                        .collect();
                node_handles.insert(
                    new_node_str.to_string(),
                    graph.add_node((new_node_str.to_string(), new_node_hashmap)),
                );
            }

            // we have no edge weights, so they are empty strings
            graph.add_edge(
                node_handles[new_node_str],
                node_handles[node_str],
                String::new(),
            );
            node_str = new_node_str;
        }
    }
    if properties_header.is_empty() {
        return graph;
    }
    let properties_iterator = properties_header.split(",");
    for property in properties_iterator {
        let re = Regex::new(r"(?P<node>[^.]*)[.](?P<property>[^=]*)[=][=](?P<value>.*)").unwrap();
        let captures = re.captures(property);
        match captures {
            Some(c) => {
                let node = c.name("node").unwrap().as_str();
                let property = c.name("property").unwrap().as_str();
                let value = c.name("value").unwrap().as_str();
                // we may propagate values that aren't in the path, because we've visited
                // those nodes as requests but not yet as responses
                if node_handles.contains_key(node) {
                    graph
                        .node_weight_mut(node_handles[node])
                        .unwrap()
                        .1
                        .insert(property.to_string(), value.to_string());
                }
            }
            None => {
                print!(
                    "WARNING:  propagating badly formed properties found in: {0}",
                    property
                );
            }
        }
    }
    return graph;
}

pub fn get_node_with_id(
    graph: &Graph<(String, HashMap<String, String>), String>,
    node_name: String,
) -> Option<NodeIndex> {
    for index in graph.node_indices() {
        if &graph.node_weight(index).unwrap().0 == &node_name {
            return Some(index);
        }
    }
    None
}

pub fn get_tree_height(
    graph: &Graph<(String, HashMap<String, String>), String>,
    root: Option<NodeIndex>,
) -> u32 {
    let starting_point;
    if !root.is_none() {
        starting_point = root.unwrap()
    } else {
        // The root of the tree by definition has no incoming edges
        let sorted = toposort(graph, None).unwrap();
        starting_point = sorted[0];
    }
    let node_map = dijkstra(graph, starting_point, None, |_| 1);
    let mut max = 0;
    for key in node_map.keys() {
        if node_map[key] > max {
            max = node_map[key];
        }
    }
    return max;
}

pub fn get_out_degree(
    graph: &Graph<(String, HashMap<String, String>), String>,
    root: Option<NodeIndex>,
) -> u32 {
    let starting_point;
    if !root.is_none() {
        starting_point = root.unwrap()
    } else {
        // The root of the tree by definition has no incoming edges
        let sorted = toposort(graph, None).unwrap();
        starting_point = sorted[0];
    }
    return graph.neighbors(starting_point).count() as u32;
}

pub fn find_leaves(
    node: NodeIndex,
    graph: &Graph<(String, HashMap<String, String>), String>,
) -> Vec<NodeIndex> {
    let mut post_order = DfsPostOrder::new(&graph, node);
    let mut to_return = Vec::new();
    while let Some(visited) = post_order.next(&graph) {
        let neighbors: Vec<NodeIndex> = graph.neighbors(visited).collect();
        if neighbors.len() == 0 {
            to_return.push(visited);
        }
    }
    return to_return;
}

pub fn find_root(graph: &Graph<(String, HashMap<String, String>), String>) -> NodeIndex {
    for node in graph.node_indices() {
        let neighbors: Vec<NodeIndex> = graph.neighbors_directed(node, Incoming).collect();
        if neighbors.len() == 0 {
            return node;
        }
    }
    panic!("no root found");
}

pub fn has_property_subset(
    property_set_1: &HashMap<String, String>, // set
    property_set_2: &HashMap<String, String>, // subset
) -> bool {
    print!("property set 1 has {:?} keys and property set 2 has {:?} keys\n", property_set_1.keys().len(), property_set_2.keys().len());
    for property in property_set_2.keys() {
        if !property_set_1.contains_key(property) { return false; }
        if property_set_1[property] != property_set_2[property] { return false; }
    }
    return true;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_small_trace_graph() -> Graph<(String, HashMap<String, String>), String> {
        let graph_string = String::from("0;1;2");
        let graph = generate_trace_graph_from_headers(graph_string, String::new());
        graph
    }

    fn make_small_target_graph() -> Graph<(String, HashMap<String, String>), String> {
        let a = String::from("a");
        let b = String::from("b");
        let c = String::from("c");
        let vertices = vec![a.clone(), b.clone(), c.clone()];
        let edges = vec![(a.clone(), b.clone()), (b.clone(), c.clone())];
        let mut ids_to_properties = HashMap::new();

        let mut a_hashmap = HashMap::new();
        a_hashmap.insert("node.metadata.WORKLOAD_NAME".to_string(), "a".to_string());
        ids_to_properties.insert("a".to_string(), a_hashmap);

        let mut b_hashmap = HashMap::new();
        b_hashmap.insert("node.metadata.WORKLOAD_NAME".to_string(), "b".to_string());
        ids_to_properties.insert("b".to_string(), b_hashmap);

        let mut c_hashmap = HashMap::new();
        c_hashmap.insert("node.metadata.WORKLOAD_NAME".to_string(), "c".to_string());
        ids_to_properties.insert("c".to_string(), c_hashmap);

        assert!(ids_to_properties.keys().len() == 3);
        assert!(ids_to_properties.contains_key(&"a".to_string()));
        assert!(ids_to_properties.contains_key(&"b".to_string()));
        assert!(ids_to_properties.contains_key(&"c".to_string()));
        for vertex in &vertices {
            assert!(ids_to_properties.contains_key(vertex));
        }
        let graph = generate_target_graph(vertices, edges, ids_to_properties);
        graph
    }
    fn little_branching_graph() -> Graph<(String, HashMap<String, String>), String> {
        let mut graph = Graph::<(String, HashMap<String, String>), String>::new();
        graph.extend_with_edges(&[(0, 1), (0, 2), (0, 3), (1, 4), (3, 5)]);
        return graph;
    }

    #[test]
    fn test_generate_trace_graph_from_headers_non_branching_graph() {
        let graph = make_small_trace_graph();
        assert_eq!(graph.node_count(), 3);
        assert_eq!(graph.edge_count(), 2);
    }

    #[test]
    fn test_generate_target_graph() {
        let graph = make_small_target_graph();
        assert_eq!(graph.node_count(), 3);
        assert_eq!(graph.edge_count(), 2);
    }

    #[test]
    fn test_correctly_parse_branching_graphs() {
        let graph = generate_trace_graph_from_headers("0;1;3,2;1".to_string(), String::new());
        assert!(graph.node_count() == 4);
        for node in graph.node_indices() {
            if graph.node_weight(node).unwrap().1["node.metadata.WORKLOAD_NAME"] == "3" {
                assert!(graph.neighbors(node).count() == 1);
            }
            if graph.node_weight(node).unwrap().1["node.metadata.WORKLOAD_NAME"] == "1" {
                assert!(graph.neighbors(node).count() == 2);
            }
        }
    }

    #[test]
    fn test_generate_trace_graph_from_headers_on_empty_string() {
        let graph = generate_trace_graph_from_headers(String::new(), String::new());
        assert!(graph.node_count() == 0);
    }

    #[test]
    fn test_get_tree_height() {
        let graph = generate_trace_graph_from_headers("0;1;3,1;2".to_string(), String::new());
        assert!(get_tree_height(&graph, None) == 2);
    }

    #[test]
    fn test_get_out_degree() {
        let straight_graph =
            generate_trace_graph_from_headers("0;1;2;3;4;5;6".to_string(), String::new());
        assert!(get_out_degree(&straight_graph, None) == 1);
    }

    #[test]
    fn test_get_node_with_id() {
        let graph = generate_trace_graph_from_headers("0;1;2;3".to_string(), String::new());
        let ret = get_node_with_id(&graph, "0".to_string());
        assert!(!ret.is_none());
    }

    #[test]
    fn test_parsing_of_properties_in_trace_graph_creation() {
        let graph = generate_trace_graph_from_headers(
            "0;1;2;3".to_string(),
            "0.property==thing,".to_string(),
        );
        let ret = get_node_with_id(&graph, "0".to_string()).unwrap();
        assert!(graph.node_weight(ret).unwrap().1[&"property".to_string()] == "thing");
    }

    #[test]
    fn test_find_leaves() {
        let graph = little_branching_graph();
        let leaves = find_leaves(NodeIndex::new(0), &graph);
        let correct_leaves = vec![2, 4, 5];
        for leaf in &leaves {
            assert!(correct_leaves.contains(&leaf.index()));
        }
    }
}