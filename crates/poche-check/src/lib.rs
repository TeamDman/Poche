// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Deterministic explicit-state exploration for the strict Poche model.

use std::collections::{HashMap, VecDeque};
use std::error::Error;
use std::fmt;

use poche_model::{
    ChanceAction, Game, LegalActions, ModelAction, ModelError, Player, PlayerAction,
};

/// Stable name for a fully declared finite transition system.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CheckScope {
    /// Two players, two suits, three ranks, six cards, and the induced `1,2,1`
    /// schedule, from either prepared first dealer.
    Micro,
}

impl CheckScope {
    /// Stable scope ID carried by reports and later evidence envelopes.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Micro => "micro-2p-2s-3r-6c-schedule-1-2-1",
        }
    }

    fn initial_states(self) -> [Game; 2] {
        match self {
            Self::Micro => [Game::new(Player::Zero), Game::new(Player::One)],
        }
    }
}

/// Dense graph-local state identifier assigned in deterministic BFS order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StateId(u32);

impl StateId {
    /// Zero-based identifier.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }

    fn index(self) -> usize {
        usize::try_from(self.0).expect("u32 state ID fits usize on supported targets")
    }
}

/// Why exploration stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminationReason {
    /// The BFS queue was drained: every reachable successor was visited.
    ReachableStateSpaceExhausted,
    /// A caller-supplied diagnostic state limit was reached.
    StateLimitReached,
}

/// Deterministic exploration measurements.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExplorationStats {
    /// Number of distinct canonical states, including all initial states.
    pub states: usize,
    /// Number of explored labeled transitions, including terminal self-loops.
    pub transitions: usize,
    /// Transitions whose successor had already been discovered.
    pub duplicate_state_hits: usize,
    /// Greatest shortest-path depth from any initial state.
    pub maximum_depth: u32,
    /// Number of prepared initial states.
    pub initial_states: usize,
    /// Number of reachable strict `Finished` states.
    pub finished_states: usize,
    /// Exact stopping condition.
    pub termination: TerminationReason,
}

/// One labeled edge in discovery/action order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Edge {
    /// Predecessor state.
    pub from: StateId,
    /// Applied semantic action.
    pub action: ModelAction,
    /// Successor state.
    pub to: StateId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Predecessor {
    state: StateId,
    action: ModelAction,
}

/// Complete reachable graph and shortest-path tree.
#[derive(Clone, Debug)]
pub struct ExplicitGraph {
    scope: CheckScope,
    states: Vec<Game>,
    edges: Vec<Edge>,
    depths: Vec<u32>,
    predecessors: Vec<Option<Predecessor>>,
    initial_states: Vec<StateId>,
    stats: ExplorationStats,
}

impl ExplicitGraph {
    /// Exact named scope.
    #[must_use]
    pub const fn scope(&self) -> CheckScope {
        self.scope
    }

    /// States in deterministic discovery order.
    #[must_use]
    pub fn states(&self) -> &[Game] {
        &self.states
    }

    /// Edges in deterministic source/action order.
    #[must_use]
    pub fn edges(&self) -> &[Edge] {
        &self.edges
    }

    /// Prepared initial state IDs.
    #[must_use]
    pub fn initial_states(&self) -> &[StateId] {
        &self.initial_states
    }

    /// Exploration measurements.
    #[must_use]
    pub const fn stats(&self) -> ExplorationStats {
        self.stats
    }

    /// State by graph-local ID.
    #[must_use]
    pub fn state(&self, id: StateId) -> Option<Game> {
        self.states.get(id.index()).copied()
    }

    /// Reconstruct the canonical shortest trace to a discovered state.
    #[must_use]
    pub fn shortest_trace(&self, target: StateId) -> Option<Counterexample> {
        let target_state = self.state(target)?;
        let mut cursor = target;
        let mut states = vec![target_state];
        let mut actions = Vec::new();
        while let Some(predecessor) = self.predecessors.get(cursor.index()).copied().flatten() {
            actions.push(predecessor.action);
            cursor = predecessor.state;
            states.push(self.state(cursor)?);
        }
        states.reverse();
        actions.reverse();
        Some(Counterexample {
            target,
            depth: self.depths[target.index()],
            states,
            actions,
        })
    }
}

/// BFS controls. A limit is diagnostic and never counts as exhaustive proof.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExplorerConfig {
    /// Optional maximum distinct state count.
    pub state_limit: Option<usize>,
}

/// A shortest replayable finite counterexample prefix.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Counterexample {
    /// Violating graph state.
    pub target: StateId,
    /// Shortest number of transitions from an initial state.
    pub depth: u32,
    /// Initial state followed by every successor through the violation.
    pub states: Vec<Game>,
    /// Actions connecting adjacent `states`.
    pub actions: Vec<ModelAction>,
}

/// Failure to construct the transition graph.
#[derive(Clone, Debug)]
pub enum CheckError {
    /// Strict semantics rejected an action returned by its own legal-action API.
    LegalTransition {
        /// Source state.
        state: Game,
        /// Advertised legal action.
        action: ModelAction,
        /// Semantic rejection.
        source: ModelError,
    },
    /// More states were found than the compact state-ID representation admits.
    StateIdOverflow,
}

impl fmt::Display for CheckError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LegalTransition {
                state,
                action,
                source,
            } => write!(
                formatter,
                "legal action {action:?} failed from {state:?}: {source}"
            ),
            Self::StateIdOverflow => formatter.write_str("explicit graph exceeded u32 state IDs"),
        }
    }
}

impl Error for CheckError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::LegalTransition { source, .. } => Some(source),
            Self::StateIdOverflow => None,
        }
    }
}

/// Exhaustively enumerate every state and action reachable in `scope`.
///
/// # Errors
///
/// Returns an error if the strict model rejects an advertised legal action or
/// the graph cannot be assigned compact IDs.
pub fn explore(scope: CheckScope) -> Result<ExplicitGraph, CheckError> {
    explore_with_config(scope, ExplorerConfig::default())
}

/// Explore with an optional diagnostic state cutoff.
///
/// # Errors
///
/// Has the same semantic failure modes as [`explore`].
#[allow(
    clippy::too_many_lines,
    reason = "the BFS loop keeps discovery, predecessor, edge, and statistics updates atomic"
)]
pub fn explore_with_config(
    scope: CheckScope,
    config: ExplorerConfig,
) -> Result<ExplicitGraph, CheckError> {
    let mut states = Vec::new();
    let mut depths = Vec::new();
    let mut predecessors = Vec::new();
    let mut initial_states = Vec::new();
    let mut canonical = HashMap::new();
    let mut queue = VecDeque::new();
    for state in scope.initial_states() {
        let id = insert_state(
            state,
            0,
            None,
            &mut states,
            &mut depths,
            &mut predecessors,
            &mut canonical,
        )?;
        initial_states.push(id);
        queue.push_back(id);
    }

    let mut edges = Vec::new();
    let mut duplicate_state_hits = 0;
    let mut maximum_depth = 0;
    let mut termination = TerminationReason::ReachableStateSpaceExhausted;
    'bfs: while let Some(from) = queue.pop_front() {
        let state = states[from.index()];
        for action in legal_actions(state) {
            let transition =
                state
                    .transition(action)
                    .map_err(|source| CheckError::LegalTransition {
                        state,
                        action,
                        source,
                    })?;
            let next = transition.next;
            let to = if let Some(id) = canonical.get(&next).copied() {
                duplicate_state_hits += 1;
                id
            } else {
                if config
                    .state_limit
                    .is_some_and(|limit| states.len() >= limit)
                {
                    termination = TerminationReason::StateLimitReached;
                    break 'bfs;
                }
                let depth = depths[from.index()] + 1;
                maximum_depth = maximum_depth.max(depth);
                let id = insert_state(
                    next,
                    depth,
                    Some(Predecessor {
                        state: from,
                        action,
                    }),
                    &mut states,
                    &mut depths,
                    &mut predecessors,
                    &mut canonical,
                )?;
                queue.push_back(id);
                id
            };
            edges.push(Edge { from, action, to });
        }
    }
    let finished_states = states
        .iter()
        .filter(|state| matches!(state, Game::Finished(_)))
        .count();
    let measurements = ExplorationStats {
        states: states.len(),
        transitions: edges.len(),
        duplicate_state_hits,
        maximum_depth,
        initial_states: initial_states.len(),
        finished_states,
        termination,
    };
    Ok(ExplicitGraph {
        scope,
        states,
        edges,
        depths,
        predecessors,
        initial_states,
        stats: measurements,
    })
}

/// Find the first state violating `invariant` in BFS discovery order and return
/// its canonical shortest trace.
///
/// # Errors
///
/// Returns graph construction errors from the same deterministic transition
/// traversal as [`explore`]. If no violation exists, the complete reachable
/// state space is explored.
#[allow(
    clippy::too_many_lines,
    reason = "an early-exit BFS retains the complete shortest-path tree without constructing all edges"
)]
pub fn shortest_counterexample(
    scope: CheckScope,
    invariant: impl Fn(Game) -> bool,
) -> Result<Option<Counterexample>, CheckError> {
    let mut states = Vec::new();
    let mut depths = Vec::new();
    let mut predecessors = Vec::new();
    let mut canonical = HashMap::new();
    let mut queue = VecDeque::new();
    for state in scope.initial_states() {
        let id = insert_state(
            state,
            0,
            None,
            &mut states,
            &mut depths,
            &mut predecessors,
            &mut canonical,
        )?;
        if !invariant(state) {
            return Ok(reconstruct_counterexample(
                id,
                &states,
                &depths,
                &predecessors,
            ));
        }
        queue.push_back(id);
    }
    while let Some(from) = queue.pop_front() {
        let state = states[from.index()];
        for action in legal_actions(state) {
            let transition =
                state
                    .transition(action)
                    .map_err(|source| CheckError::LegalTransition {
                        state,
                        action,
                        source,
                    })?;
            if canonical.contains_key(&transition.next) {
                continue;
            }
            let id = insert_state(
                transition.next,
                depths[from.index()] + 1,
                Some(Predecessor {
                    state: from,
                    action,
                }),
                &mut states,
                &mut depths,
                &mut predecessors,
                &mut canonical,
            )?;
            if !invariant(transition.next) {
                return Ok(reconstruct_counterexample(
                    id,
                    &states,
                    &depths,
                    &predecessors,
                ));
            }
            queue.push_back(id);
        }
    }
    Ok(None)
}

fn reconstruct_counterexample(
    target: StateId,
    graph_states: &[Game],
    depths: &[u32],
    predecessors: &[Option<Predecessor>],
) -> Option<Counterexample> {
    let mut cursor = target;
    let mut states = vec![*graph_states.get(target.index())?];
    let mut actions = Vec::new();
    while let Some(predecessor) = predecessors.get(cursor.index()).copied().flatten() {
        actions.push(predecessor.action);
        cursor = predecessor.state;
        states.push(*graph_states.get(cursor.index())?);
    }
    states.reverse();
    actions.reverse();
    Some(Counterexample {
        target,
        depth: depths[target.index()],
        states,
        actions,
    })
}

fn legal_actions(state: Game) -> Vec<ModelAction> {
    match state.legal_actions() {
        LegalActions::Chance(actions) => actions
            .into_iter()
            .map(|action: ChanceAction| ModelAction::Chance(action))
            .collect(),
        LegalActions::Player(actions) => actions
            .into_iter()
            .map(|action: PlayerAction| ModelAction::Player(action))
            .collect(),
        LegalActions::Environment => vec![ModelAction::Settle],
        LegalActions::Finished => vec![ModelAction::Absorb],
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "parallel graph columns avoid per-state allocation and support later SCC passes"
)]
fn insert_state(
    state: Game,
    depth: u32,
    predecessor: Option<Predecessor>,
    states: &mut Vec<Game>,
    depths: &mut Vec<u32>,
    predecessors: &mut Vec<Option<Predecessor>>,
    canonical: &mut HashMap<Game, StateId>,
) -> Result<StateId, CheckError> {
    let raw_id = u32::try_from(states.len()).map_err(|_| CheckError::StateIdOverflow)?;
    let id = StateId(raw_id);
    states.push(state);
    depths.push(depth);
    predecessors.push(predecessor);
    canonical.insert(state, id);
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use poche_model::Phase;

    #[test]
    fn explicit_small_graphs_are_deterministic() {
        let config = ExplorerConfig {
            state_limit: Some(5_000),
        };
        let left = explore_with_config(CheckScope::Micro, config).unwrap();
        let right = explore_with_config(CheckScope::Micro, config).unwrap();
        assert_eq!(left.stats(), right.stats());
        assert_eq!(left.states(), right.states());
        assert_eq!(left.edges(), right.edges());
        assert_eq!(
            left.stats().termination,
            TerminationReason::StateLimitReached
        );
        assert_eq!(left.stats().states, 5_000);
        assert_eq!(left.initial_states().len(), 2);
    }

    #[test]
    fn shortest_counterexample_is_a_one_step_replay() {
        let counterexample = shortest_counterexample(CheckScope::Micro, |state| {
            state.phase() == Phase::AwaitingDeal
        })
        .unwrap()
        .expect("dealt state refutes the intentionally false invariant");
        assert_eq!(counterexample.depth, 1);
        assert_eq!(counterexample.actions.len(), 1);
        assert_eq!(counterexample.states.len(), 2);
        assert_eq!(counterexample.states[0].phase(), Phase::AwaitingDeal);
        assert_eq!(counterexample.states[1].phase(), Phase::Bidding);
        let replay = counterexample.states[0]
            .transition(counterexample.actions[0])
            .unwrap()
            .next;
        assert_eq!(replay, counterexample.states[1]);
    }
}
