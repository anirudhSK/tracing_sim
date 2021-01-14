/* This file contains functions relating to creating and comparing trace and target (user-given) graphs */

extern crate petgraph;
use petgraph::graph::{Graph, NodeIndex};
use petgraph::algo::toposort;
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

pub fn generate_target_graph(vertices: Vec<String>,
                            edges: Vec<(String, String)>,
                            ids_to_properties: HashMap<String, Vec<String>>)
                           -> Graph<String, String> {
    let mut graph = Graph::new();

    // In order to make edges, we have to know the handles of the nodes, and you 
    // get the handles of the nodes by adding them to the graph

    let mut nodes_to_node_handles: HashMap<String, NodeIndex> = HashMap::new();
    for node in vertices {
        nodes_to_node_handles.insert(node.clone(), graph.add_node(node));
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
 *  The trace is represented in paths_header as a string where the first node is 
 *  the root.  Thus "0 1 2" is a graph that looks like 0 -> 1 -> 2 with 0 as root.
 *
 *  Arguments:
 *  @paths_header:  the string version of the trace, generated by the tracing simulator
 *
 *  Return Value:
 *  @graph:  A petgraph graph representation of the same trace
 */
pub fn generate_trace_graph_from_headers<'a>(paths_header: String) -> Graph<String, String> {
    let mut graph = Graph::new();
    let mut nodes_iterator2 = paths_header.split_whitespace();
    let mut first_node: String = String::new();
    first_node.push_str(nodes_iterator2.next().unwrap());
    let mut first_node_handle = graph.add_node(first_node);
    for node in nodes_iterator2 {
        let mut node_as_string = String::new();
        node_as_string.push_str(node);
        let second_node_handle = graph.add_node(node_as_string);

        graph.add_edge(first_node_handle, second_node_handle, String::new());
        first_node_handle = second_node_handle;
    }
    graph
}



/* Note:  the more efficient algorithm to do this (that is also used by the boost library) is here:
 * https://citeseerx.ist.psu.edu/viewdoc/download;jsessionid=E6BEA4B7B3694938A0BBEBB3604F14C7?doi=10.1.1.101.5342&rep=rep1&type=pdf
 * But that's more complicated than we need right now for a prototype, so the below algorithm 
 * does subgraph isomorphism only on non-branching trees.  So... they're always subgraph isomorphic, unless target is bigger than trace.
 * 
 * All this algorithm does is check the length, and if so, create a mapping between the trace and target graphs
 * Arguments:
 * @trace_graph: the graph of the trace observed
 * @target_graph: the graph of the target pattern we want to match to
 *
 * Return value:
 * @mapping: a hashmap mapping vertices in target_graph to those in trace_graph if the graphs are subgraph isomophic, and
 *           an empty hashmap otherwise
 */
pub fn get_sub_graph_mapping(trace_graph:  Graph<String, String>, target_graph: Graph<String, String>) -> HashMap<NodeIndex, NodeIndex> {
    // Right now, simply having more nodes than the target will be sufficient to say yes because
    // I haven't implemented branching.  So that's what we're going to do, and we'll make this more general later
    let mut mapping = HashMap::new();
    if trace_graph.node_count() >= target_graph.node_count() {
        let trace_graph_order = toposort(&trace_graph, None).unwrap();
        let target_graph_order = toposort(&target_graph, None).unwrap();
        let trace_root = trace_graph_order[0];
        let target_root = target_graph_order[0];
        mapping.insert(trace_root, target_root);
        let mut trace_children: Vec<NodeIndex> = trace_graph.neighbors(trace_root).collect();
        let mut target_children: Vec<NodeIndex> = trace_graph.neighbors(target_root).collect();
        while trace_children.len() != 0 && target_children.len() != 0 {
            let trace_child = trace_children[0];
            let target_child = target_children[0];
            mapping.insert(target_child, trace_child);
            trace_children = trace_graph.neighbors(trace_child).collect();
            target_children = trace_graph.neighbors(target_child).collect();
        }
    }
    mapping
}



#[cfg(test)]
mod tests {
    use super::*;

    fn make_small_trace_graph() -> Graph<String, String> {
        let graph_string = String::from("0 1 2");
        let graph = generate_trace_graph_from_headers(graph_string);
        graph
    }


    fn make_small_target_graph() -> Graph<String, String> {
        let a = String::from("a");
        let b = String::from("b");
        let c = String::from("c");
        let mut vertices = vec![ a.clone(), b.clone(), c.clone()];
        let mut edges = vec![(a.clone(), b.clone()), (b.clone(), c.clone())];
        let mut ids_to_properties = HashMap::new();
        for vertex in vertices.clone() {
            ids_to_properties.insert(vertex.clone(), Vec::new());
        }
        let graph = generate_target_graph(vertices, edges, ids_to_properties);
        graph
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
    fn test_get_subgraph_mapping_with_single_child_graphs() {
        let trace_graph = make_small_trace_graph();
        let target_graph = make_small_target_graph();
        let mapping = get_sub_graph_mapping(trace_graph, target_graph);
        assert_eq!(mapping.len(), 3); 
    }

}