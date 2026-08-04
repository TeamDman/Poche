use std::collections::VecDeque;
use std::error::Error;
use std::fmt;

use poche_model::{Game, ModelAction, Phase, RoundId};

use crate::{Counterexample, Edge, ExplicitGraph, StateId};

/// One real or injected temporal edge. Injected edges have no model action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TemporalEdge {
    /// Source state.
    pub from: StateId,
    /// Successor state.
    pub to: StateId,
    /// Strict semantic action, absent only for a controlled temporal mutation.
    pub action: Option<ModelAction>,
}

/// Replayable finite prefix followed by a closed repeating cycle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lasso {
    /// Shortest prefix from a prepared initial state to the cycle entry.
    pub prefix: Counterexample,
    /// Closed state sequence; first and last IDs are equal.
    pub cycle_states: Vec<StateId>,
    /// Edges connecting adjacent `cycle_states`.
    pub cycle_edges: Vec<TemporalEdge>,
}

/// Deadlock, SCC, progress, and universal-termination result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LivenessReport {
    /// Reachable nonterminal states with no successor.
    pub nonterminal_deadlocks: Vec<StateId>,
    /// Terminal states without exactly the semantic absorb self-loop.
    pub terminal_edge_violations: Vec<StateId>,
    /// Total strongly connected components.
    pub strongly_connected_components: usize,
    /// Components containing at least one directed cycle.
    pub cyclic_components: usize,
    /// Cyclic components containing a nonterminal state.
    pub nonterminal_cyclic_components: usize,
    /// Edges that fail the exact countdown rank obligation.
    pub progress_violations: Vec<TemporalEdge>,
    /// Greatest remaining-step rank in the reachable graph.
    pub maximum_progress_rank: u8,
    /// Whether every path from both initial states must reach `Finished`.
    pub universal_termination: bool,
    /// First nonterminal cycle, when universal termination fails.
    pub lasso: Option<Lasso>,
}

/// Invalid injected temporal defect or inconsistent explicit graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LivenessError(String);

impl fmt::Display for LivenessError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for LivenessError {}

/// Analyze the complete raw explicit graph for deadlocks and infinite
/// nonterminal paths.
///
/// # Errors
///
/// Returns an error only if graph-local IDs/edges are internally inconsistent.
pub fn analyze_liveness(graph: &ExplicitGraph) -> Result<LivenessReport, LivenessError> {
    analyze(graph, None, None)
}

/// Inject one illegal nonterminal self-loop and prove the liveness checker
/// returns a replayable lasso.
///
/// # Errors
///
/// Rejects an absent or already-terminal injection state.
pub fn analyze_with_nonterminal_stutter(
    graph: &ExplicitGraph,
    state: StateId,
) -> Result<LivenessReport, LivenessError> {
    let game = graph
        .state(state)
        .ok_or_else(|| LivenessError("stutter state is absent".to_owned()))?;
    if game.phase() == Phase::Finished {
        return Err(LivenessError(
            "controlled stutter must target a nonterminal state".to_owned(),
        ));
    }
    analyze(graph, Some((state, state)), None)
}

/// Remove every successor from one reachable nonterminal state and prove the
/// liveness checker reports that exact deadlock.
///
/// # Errors
///
/// Rejects an absent or already-terminal mutation state.
pub fn analyze_with_nonterminal_deadlock(
    graph: &ExplicitGraph,
    state: StateId,
) -> Result<LivenessReport, LivenessError> {
    let game = graph
        .state(state)
        .ok_or_else(|| LivenessError("deadlock state is absent".to_owned()))?;
    if game.phase() == Phase::Finished {
        return Err(LivenessError(
            "controlled deadlock must target a nonterminal state".to_owned(),
        ));
    }
    analyze(graph, None, Some(state))
}

/// Exact number of semantic actions remaining on every continuation from this
/// phase-specific state. It is 20 at either prepared initial state, decreases
/// by one on every nonterminal edge, and is zero in `Finished`.
#[must_use]
pub fn progress_rank(game: Game) -> u8 {
    match game {
        Game::AwaitingDeal(state) => {
            future_round_actions(state.ledger().round())
                + 4
                + 2 * state.ledger().round().hand_size()
        }
        Game::Bidding(state) => {
            let bids = state.bids();
            let bids_remaining = u8::from(bids[0].is_none()) + u8::from(bids[1].is_none());
            future_round_actions(state.ledger().round())
                + bids_remaining
                + 2 * state.ledger().round().hand_size()
                + 1
        }
        Game::Playing(state) => {
            future_round_actions(state.ledger().round())
                + state.hand(poche_model::Player::Zero).len()
                + state.hand(poche_model::Player::One).len()
                + 1
        }
        Game::Scoring(state) => future_round_actions(state.ledger().round()) + 1,
        Game::Finished(_) => 0,
    }
}

const fn future_round_actions(round: RoundId) -> u8 {
    match round {
        RoundId::OneAscending => 14,
        RoundId::Two => 6,
        RoundId::OneDescending => 0,
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "the report combines the same CSR edge set's deadlock, SCC, cycle, and progress evidence"
)]
fn analyze(
    graph: &ExplicitGraph,
    injected_edge: Option<(StateId, StateId)>,
    removed_successors: Option<StateId>,
) -> Result<LivenessReport, LivenessError> {
    let adjacency = Csr::from_graph(graph, injected_edge, removed_successors)?;
    let mut nonterminal_deadlocks = Vec::new();
    let mut terminal_edge_violations = Vec::new();
    let mut progress_violations = Vec::new();
    let mut maximum_progress_rank = 0;
    for (index, game) in graph.states().iter().copied().enumerate() {
        let id = state_id(index)?;
        let outgoing = adjacency.neighbors(id);
        maximum_progress_rank = maximum_progress_rank.max(progress_rank(game));
        if game.phase() == Phase::Finished {
            let valid = outgoing.len() == 1
                && outgoing[0] == id
                && graph.edges().iter().any(|edge| {
                    edge.from == id && edge.to == id && edge.action == ModelAction::Absorb
                });
            if !valid {
                terminal_edge_violations.push(id);
            }
        } else if outgoing.is_empty() {
            nonterminal_deadlocks.push(id);
        }
        for to in outgoing.iter().copied() {
            let target = graph
                .state(to)
                .ok_or_else(|| LivenessError("edge target is absent".to_owned()))?;
            let valid = if game.phase() == Phase::Finished {
                to == id && progress_rank(game) == 0 && progress_rank(target) == 0
            } else {
                progress_rank(game) == progress_rank(target) + 1
            };
            if !valid {
                progress_violations.push(TemporalEdge {
                    from: id,
                    to,
                    action: find_action(graph, id, to),
                });
            }
        }
    }

    let scc = strongly_connected_components(&adjacency);
    let mut cyclic = vec![false; scc.sizes.len()];
    let mut nonterminal = vec![false; scc.sizes.len()];
    for (component, size) in scc.sizes.iter().copied().enumerate() {
        cyclic[component] = size > 1;
    }
    for (index, game) in graph.states().iter().copied().enumerate() {
        let component = scc.component_of[index];
        nonterminal[component] |= game.phase() != Phase::Finished;
        let id = state_id(index)?;
        if adjacency.neighbors(id).contains(&id) {
            cyclic[component] = true;
        }
    }
    let cyclic_components = cyclic.iter().filter(|value| **value).count();
    let nonterminal_cyclic_components = cyclic
        .iter()
        .zip(&nonterminal)
        .filter(|(cycle, live)| **cycle && **live)
        .count();
    let lasso_component = cyclic
        .iter()
        .zip(&nonterminal)
        .position(|(cycle, live)| *cycle && *live);
    let lasso = lasso_component
        .map(|component| build_lasso(graph, &adjacency, &scc, component))
        .transpose()?;
    let universal_termination = nonterminal_deadlocks.is_empty()
        && terminal_edge_violations.is_empty()
        && nonterminal_cyclic_components == 0;
    Ok(LivenessReport {
        nonterminal_deadlocks,
        terminal_edge_violations,
        strongly_connected_components: scc.sizes.len(),
        cyclic_components,
        nonterminal_cyclic_components,
        progress_violations,
        maximum_progress_rank,
        universal_termination,
        lasso,
    })
}

fn build_lasso(
    graph: &ExplicitGraph,
    adjacency: &Csr,
    scc: &SccResult,
    component: usize,
) -> Result<Lasso, LivenessError> {
    let entry_index = scc
        .component_of
        .iter()
        .position(|candidate| *candidate == component)
        .ok_or_else(|| LivenessError("cyclic component has no member".to_owned()))?;
    let entry = state_id(entry_index)?;
    let cycle_states = if adjacency.neighbors(entry).contains(&entry) {
        vec![entry, entry]
    } else {
        let next = adjacency
            .neighbors(entry)
            .iter()
            .copied()
            .find(|target| scc.component_of[target.index()] == component)
            .ok_or_else(|| LivenessError("SCC entry has no internal edge".to_owned()))?;
        let path = path_within_component(adjacency, scc, component, next, entry)?;
        let mut cycle = vec![entry];
        cycle.extend(path);
        cycle
    };
    let cycle_edges = cycle_states
        .windows(2)
        .map(|pair| TemporalEdge {
            from: pair[0],
            to: pair[1],
            action: find_action(graph, pair[0], pair[1]),
        })
        .collect();
    let prefix = graph
        .shortest_trace(entry)
        .ok_or_else(|| LivenessError("cycle entry is not reachable".to_owned()))?;
    Ok(Lasso {
        prefix,
        cycle_states,
        cycle_edges,
    })
}

fn path_within_component(
    adjacency: &Csr,
    scc: &SccResult,
    component: usize,
    start: StateId,
    target: StateId,
) -> Result<Vec<StateId>, LivenessError> {
    let mut predecessors = vec![None; adjacency.states()];
    let mut queue = VecDeque::from([start]);
    predecessors[start.index()] = Some(start);
    while let Some(state) = queue.pop_front() {
        if state == target {
            let mut path = vec![target];
            let mut cursor = target;
            while cursor != start {
                cursor = predecessors[cursor.index()]
                    .ok_or_else(|| LivenessError("cycle path predecessor absent".to_owned()))?;
                path.push(cursor);
            }
            path.reverse();
            return Ok(path);
        }
        for next in adjacency.neighbors(state).iter().copied() {
            if scc.component_of[next.index()] == component && predecessors[next.index()].is_none() {
                predecessors[next.index()] = Some(state);
                queue.push_back(next);
            }
        }
    }
    Err(LivenessError(
        "strong component did not contain a return path".to_owned(),
    ))
}

fn find_action(graph: &ExplicitGraph, from: StateId, to: StateId) -> Option<ModelAction> {
    graph
        .edges()
        .iter()
        .find(|edge| edge.from == from && edge.to == to)
        .map(|edge| edge.action)
}

struct Csr {
    offsets: Vec<usize>,
    targets: Vec<StateId>,
}

impl Csr {
    fn from_graph(
        graph: &ExplicitGraph,
        injected: Option<(StateId, StateId)>,
        removed_successors: Option<StateId>,
    ) -> Result<Self, LivenessError> {
        let states = graph.states().len();
        let mut counts = vec![0_usize; states];
        for edge in graph.edges() {
            if Some(edge.from) == removed_successors {
                continue;
            }
            let count = counts
                .get_mut(edge.from.index())
                .ok_or_else(|| LivenessError("edge source is absent".to_owned()))?;
            *count = count
                .checked_add(1)
                .ok_or_else(|| LivenessError("edge count overflow".to_owned()))?;
        }
        if let Some((from, to)) = injected {
            if from.index() >= states || to.index() >= states {
                return Err(LivenessError("injected edge endpoint is absent".to_owned()));
            }
            counts[from.index()] += 1;
        }
        Self::from_counts_and_edges(states, &counts, graph.edges(), injected, removed_successors)
    }

    fn from_counts_and_edges(
        states: usize,
        counts: &[usize],
        edges: &[Edge],
        injected: Option<(StateId, StateId)>,
        removed_successors: Option<StateId>,
    ) -> Result<Self, LivenessError> {
        let mut offsets = vec![0_usize; states + 1];
        for index in 0..states {
            offsets[index + 1] = offsets[index]
                .checked_add(counts[index])
                .ok_or_else(|| LivenessError("CSR offset overflow".to_owned()))?;
        }
        let mut targets = vec![StateId(0); offsets[states]];
        let mut cursors = offsets[..states].to_vec();
        for edge in edges {
            if Some(edge.from) == removed_successors {
                continue;
            }
            targets[cursors[edge.from.index()]] = edge.to;
            cursors[edge.from.index()] += 1;
        }
        if let Some((from, to)) = injected {
            targets[cursors[from.index()]] = to;
        }
        Ok(Self { offsets, targets })
    }

    fn states(&self) -> usize {
        self.offsets.len() - 1
    }

    fn neighbors(&self, state: StateId) -> &[StateId] {
        &self.targets[self.offsets[state.index()]..self.offsets[state.index() + 1]]
    }

    #[cfg(test)]
    fn from_pairs(states: usize, edges: &[(u32, u32)]) -> Self {
        let edges = edges
            .iter()
            .map(|(from, to)| Edge {
                from: StateId(*from),
                action: ModelAction::Absorb,
                to: StateId(*to),
            })
            .collect::<Vec<_>>();
        let mut counts = vec![0; states];
        for edge in &edges {
            counts[edge.from.index()] += 1;
        }
        Self::from_counts_and_edges(states, &counts, &edges, None, None).unwrap()
    }
}

struct SccResult {
    component_of: Vec<usize>,
    sizes: Vec<usize>,
}

fn strongly_connected_components(adjacency: &Csr) -> SccResult {
    let states = adjacency.states();
    let mut tarjan = Tarjan {
        next_index: 0,
        indices: vec![usize::MAX; states],
        low_links: vec![0; states],
        stack: Vec::new(),
        on_stack: vec![false; states],
        component_of: vec![usize::MAX; states],
        sizes: Vec::new(),
    };
    for state in 0..states {
        if tarjan.indices[state] == usize::MAX {
            tarjan.visit(
                StateId(u32::try_from(state).expect("graph IDs fit u32")),
                adjacency,
            );
        }
    }
    SccResult {
        component_of: tarjan.component_of,
        sizes: tarjan.sizes,
    }
}

struct Tarjan {
    next_index: usize,
    indices: Vec<usize>,
    low_links: Vec<usize>,
    stack: Vec<StateId>,
    on_stack: Vec<bool>,
    component_of: Vec<usize>,
    sizes: Vec<usize>,
}

impl Tarjan {
    fn visit(&mut self, state: StateId, adjacency: &Csr) {
        let index = state.index();
        self.indices[index] = self.next_index;
        self.low_links[index] = self.next_index;
        self.next_index += 1;
        self.stack.push(state);
        self.on_stack[index] = true;

        for next in adjacency.neighbors(state).iter().copied() {
            let next_index = next.index();
            if self.indices[next_index] == usize::MAX {
                self.visit(next, adjacency);
                self.low_links[index] = self.low_links[index].min(self.low_links[next_index]);
            } else if self.on_stack[next_index] {
                self.low_links[index] = self.low_links[index].min(self.indices[next_index]);
            }
        }
        if self.low_links[index] == self.indices[index] {
            let component = self.sizes.len();
            let mut size = 0;
            loop {
                let member = self.stack.pop().expect("SCC root remains on stack");
                self.on_stack[member.index()] = false;
                self.component_of[member.index()] = component;
                size += 1;
                if member == state {
                    break;
                }
            }
            self.sizes.push(size);
        }
    }
}

fn state_id(index: usize) -> Result<StateId, LivenessError> {
    Ok(StateId(u32::try_from(index).map_err(|_| {
        LivenessError("state index exceeds u32".to_owned())
    })?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exhaustive_test_graph;

    #[test]
    fn liveness_small_graphs_distinguish_cycles_and_deadlocks() {
        let graph = Csr::from_pairs(4, &[(0, 1), (1, 0), (1, 2), (2, 2)]);
        let scc = strongly_connected_components(&graph);
        assert_eq!(scc.sizes.len(), 3);
        assert!(scc.sizes.contains(&2));
        assert!(graph.neighbors(StateId(3)).is_empty());
        assert_eq!(graph.neighbors(StateId(2)), &[StateId(2)]);
    }

    #[test]
    fn nonterminating_lasso_detects_injected_stuttering() {
        let graph = exhaustive_test_graph();
        let initial = graph.initial_states()[0];
        let report = analyze_with_nonterminal_stutter(graph, initial).unwrap();
        assert!(!report.universal_termination);
        assert_eq!(report.nonterminal_cyclic_components, 1);
        assert_eq!(report.progress_violations.len(), 1);
        let lasso = report.lasso.expect("stuttering creates a lasso");
        assert_eq!(lasso.prefix.depth, 0);
        assert_eq!(lasso.cycle_states, vec![initial, initial]);
        assert_eq!(lasso.cycle_edges.len(), 1);
        assert_eq!(lasso.cycle_edges[0].action, None);
    }

    #[test]
    fn nonterminal_deadlock_detects_removed_successors() {
        let graph = exhaustive_test_graph();
        let initial = graph.initial_states()[0];
        let report = analyze_with_nonterminal_deadlock(graph, initial).unwrap();
        assert!(!report.universal_termination);
        assert_eq!(report.nonterminal_deadlocks, vec![initial]);
        assert_eq!(report.nonterminal_cyclic_components, 0);
        assert!(report.lasso.is_none());
    }
}
