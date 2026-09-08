//! Deterministic service dependency graph cycle algorithms.

use std::collections::{BTreeMap, BTreeSet};

use super::{DirectedServiceEdge, ServiceCycle, ServiceId, ServiceRelationGraph};

#[must_use]
pub fn detect_ordering_cycles<'a, I>(services: I) -> Vec<ServiceCycle>
where
    I: IntoIterator<Item = (&'a ServiceId, &'a ServiceRelationGraph)>,
{
    let mut edges = Vec::new();
    for (origin, graph) in services {
        edges.extend(graph.directed_ordering_edges(origin));
    }
    detect_directed_cycles(&edges)
}

#[must_use]
pub fn is_ordering_acyclic<'a, I>(services: I) -> bool
where
    I: IntoIterator<Item = (&'a ServiceId, &'a ServiceRelationGraph)>,
{
    detect_ordering_cycles(services).is_empty()
}

#[must_use]
pub fn detect_requirement_cycles<'a, I>(services: I, strict_only: bool) -> Vec<ServiceCycle>
where
    I: IntoIterator<Item = (&'a ServiceId, &'a ServiceRelationGraph)>,
{
    let mut edges = Vec::new();
    for (origin, graph) in services {
        for edge in graph.directed_requirement_edges(origin) {
            if !strict_only || edge.is_strict() {
                edges.push(edge);
            }
        }
    }
    detect_directed_cycles(&edges)
}

#[must_use]
pub fn is_requirement_acyclic<'a, I>(services: I, strict_only: bool) -> bool
where
    I: IntoIterator<Item = (&'a ServiceId, &'a ServiceRelationGraph)>,
{
    detect_requirement_cycles(services, strict_only).is_empty()
}

#[must_use]
pub fn detect_directed_cycles(edges: &[DirectedServiceEdge]) -> Vec<ServiceCycle> {
    let mut adj: BTreeMap<ServiceId, BTreeSet<ServiceId>> = BTreeMap::new();
    for edge in edges {
        adj.entry(edge.source.clone())
            .or_default()
            .insert(edge.target.clone());
        adj.entry(edge.target.clone()).or_default();
    }

    let nodes: Vec<ServiceId> = adj.keys().cloned().collect();
    let mut cycles = Vec::new();

    for start_node in &nodes {
        let mut path_stack = vec![start_node.clone()];
        let mut in_path = BTreeSet::new();
        in_path.insert(start_node.clone());

        fn find_cycles_from(
            start: &ServiceId,
            current: &ServiceId,
            adj: &BTreeMap<ServiceId, BTreeSet<ServiceId>>,
            path_stack: &mut Vec<ServiceId>,
            in_path: &mut BTreeSet<ServiceId>,
            cycles: &mut Vec<ServiceCycle>,
        ) {
            const MAX_DETECTED_CYCLES: usize = 128;
            if cycles.len() >= MAX_DETECTED_CYCLES {
                return;
            }
            if let Some(neighbors) = adj.get(current) {
                for neighbor in neighbors {
                    if neighbor == start {
                        let mut cycle_path = path_stack.clone();
                        cycle_path.push(start.clone());
                        cycles.push(ServiceCycle::new(cycle_path));
                        if cycles.len() >= MAX_DETECTED_CYCLES {
                            return;
                        }
                    } else if neighbor > start && !in_path.contains(neighbor) {
                        in_path.insert(neighbor.clone());
                        path_stack.push(neighbor.clone());
                        find_cycles_from(start, neighbor, adj, path_stack, in_path, cycles);
                        path_stack.pop();
                        in_path.remove(neighbor);
                    }
                }
            }
        }

        find_cycles_from(
            start_node,
            start_node,
            &adj,
            &mut path_stack,
            &mut in_path,
            &mut cycles,
        );
        if cycles.len() >= 128 {
            break;
        }
    }
    cycles
}
