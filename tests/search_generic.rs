//! Public search-only consumers and neutral engine regression properties.
//!
//! All states and actions are unrelated to cards. These tests must compile with
//! only the dependency-free `search` feature, without any Pyramid adapter.

#![cfg(feature = "search")]

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use xsar::search::{
    SearchLimits, SearchMethod, SearchOutcome, SearchProblem, SearchStats, SearchStop, solve,
};


/// Unrelated graph key with no domain-specific bit fields.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct Vertex(u16);


/// Distinct graph edge labels for predecessor-chain and tie-order assertions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Hop {
    /// Ordinary edge through the first branch.
    Forward,
    /// Alternative first branch.
    Detour,
    /// Edge that improves the previously discovered cost of a state.
    Shortcut,
}


/// Small graph carrying stable edges and optional A* guidance.
struct Graph {
    /// Directed labelled edges in the problem's deterministic order.
    edges: Vec<(Vertex, Vertex, Hop)>,
    /// Optional lower-bound estimates by vertex.
    estimates: Vec<(Vertex, u32)>,
    /// Sole goal vertex.
    goal: Vertex,
    /// Optional vertices declared impossible by the test.
    pruned: Vec<Vertex>,
}


impl SearchProblem for Graph {
    type State = Vertex;
    type Action = Hop;
    type Successors = std::vec::IntoIter<(Hop, Vertex)>;

    /// Begin every small regression graph at vertex one.
    fn initial(&self) -> Vertex {
        Vertex(1)
    }


    /// Match the supplied graph goal without using any other domain.
    fn is_goal(&self, state: Vertex) -> bool {
        state == self.goal
    }


    /// Preserve the explicit order of outgoing edges.
    fn successors(&self, state: Vertex) -> Self::Successors {
        self.edges
            .iter()
            .filter_map(|(from, next, action)| (*from == state).then_some((*action, *next)))
            .collect::<Vec<_>>()
            .into_iter()
    }


    /// Return an explicitly supplied estimate or conservative zero.
    fn heuristic(&self, state: Vertex) -> u32 {
        self.estimates
            .iter()
            .find_map(|(key, value)| (*key == state).then_some(*value))
            .unwrap_or(0)
    }


    /// Apply supplied pruning labels, including deliberately false test labels.
    fn impossible(&self, state: Vertex) -> bool {
        self.pruned.contains(&state)
    }
}


/// Supply ample small-graph budgets without relying on workstation defaults.
fn generous_limits() -> SearchLimits {
    SearchLimits {
        max_states: 1000,
        max_expansions: 1000,
        max_memory_bytes: 16 * 1024 * 1024,
        max_duration: Duration::from_secs(10),
    }
}


/// Build one unambiguous edge and a configurable possibly unreachable goal.
fn single_edge(goal: u16) -> Graph {
    Graph {
        edges: vec![(Vertex(1), Vertex(2), Hop::Forward)],
        estimates: Vec::new(),
        goal: Vertex(goal),
        pruned: Vec::new(),
    }
}


/// Extract a solved route or report the unexpected outcome in test diagnostics.
fn solved_actions(outcome: SearchOutcome<Hop>) -> Box<[Hop]> {
    match outcome {
        SearchOutcome::Solved { actions, .. } => actions,
        other => panic!("expected a solved graph, received {other:?}"),
    }
}


/// Cancellation must precede all zero resource limits and even initial-goal work.
#[test]
fn pre_cancel_is_reported_before_zero_limits() {
    let limits = SearchLimits {
        max_states: 0,
        max_expansions: 0,
        max_memory_bytes: 0,
        max_duration: Duration::ZERO,
    };
    let result = solve(
        &single_edge(1),
        limits,
        &AtomicBool::new(true),
        SearchMethod::AStar,
    );
    assert!(matches!(
        result,
        SearchOutcome::Stopped {
            reason: SearchStop::Cancelled,
            ..
        }
    ));
}


/// Each zero budget stops distinctly without manufacturing unsolvability proof.
#[test]
fn zero_resource_limits_have_distinct_outcomes() {
    for (kind, reason) in [
        (0, SearchStop::StateLimit),
        (1, SearchStop::ExpansionLimit),
        (2, SearchStop::MemoryLimit),
        (3, SearchStop::TimeLimit),
    ] {
        let mut limits = generous_limits();
        match kind {
            0 => limits.max_states = 0,
            1 => limits.max_expansions = 0,
            2 => limits.max_memory_bytes = 0,
            _ => limits.max_duration = Duration::ZERO,
        }

        let result = solve(
            &single_edge(2),
            limits,
            &AtomicBool::new(false),
            SearchMethod::AStar,
        );
        assert!(matches!(
            result,
            SearchOutcome::Stopped { reason: actual, .. } if actual == reason
        ));
    }
}


/// Insertion cutoffs retain their resource reason instead of claiming no route.
#[test]
fn state_limit_during_successor_insertion_is_not_unsolvable() {
    let mut limits = generous_limits();
    limits.max_states = 1;
    let result = solve(
        &single_edge(2),
        limits,
        &AtomicBool::new(false),
        SearchMethod::AStar,
    );
    assert!(matches!(
        result,
        SearchOutcome::Stopped {
            reason: SearchStop::StateLimit,
            stats: SearchStats { states: 1, .. },
        }
    ));
}


/// Complete frontier exhaustion yields an unsolvable reference result.
#[test]
fn exhausted_frontier_is_proven_unsolvable() {
    let result = solve(
        &single_edge(3),
        generous_limits(),
        &AtomicBool::new(false),
        SearchMethod::Dijkstra,
    );
    assert!(matches!(
        result,
        SearchOutcome::Unsolvable {
            stats: SearchStats {
                expanded: 2,
                states: 2,
                ..
            },
        }
    ));
}


/// Reference search disregards intentionally false pruning and huge estimates.
#[test]
fn reference_disables_pruning_and_heuristic() {
    let mut graph = single_edge(2);
    graph.pruned = vec![Vertex(1), Vertex(2)];
    graph.estimates = vec![(Vertex(1), u32::MAX), (Vertex(2), u32::MAX)];
    assert_eq!(
        &*solved_actions(solve(
            &graph,
            generous_limits(),
            &AtomicBool::new(false),
            SearchMethod::Dijkstra,
        )),
        &[Hop::Forward],
    );
}


/// Improved costs leave earlier parent chains immutable and reconstruct correctly.
#[test]
fn reopening_retains_immutable_predecessor_chains() {
    let graph = Graph {
        edges: vec![
            (Vertex(1), Vertex(2), Hop::Forward),
            (Vertex(1), Vertex(5), Hop::Detour),
            (Vertex(2), Vertex(3), Hop::Forward),
            (Vertex(3), Vertex(4), Hop::Forward),
            (Vertex(5), Vertex(4), Hop::Shortcut),
            (Vertex(4), Vertex(6), Hop::Forward),
            (Vertex(6), Vertex(7), Hop::Forward),
            (Vertex(7), Vertex(8), Hop::Forward),
            (Vertex(8), Vertex(9), Hop::Forward),
        ],
        estimates: vec![(Vertex(5), 4)],
        goal: Vertex(9),
        pruned: Vec::new(),
    };
    let expected = [
        Hop::Detour,
        Hop::Shortcut,
        Hop::Forward,
        Hop::Forward,
        Hop::Forward,
        Hop::Forward,
    ];
    for method in [SearchMethod::AStar, SearchMethod::Dijkstra] {
        let actions = solved_actions(solve(
            &graph,
            generous_limits(),
            &AtomicBool::new(false),
            method,
        ));
        assert_eq!(&*actions, &expected);
    }
}


/// Stable ties follow successor order rather than randomised map iteration.
#[test]
fn equal_cost_ties_are_deterministic() {
    let graph = Graph {
        edges: vec![
            (Vertex(1), Vertex(2), Hop::Forward),
            (Vertex(1), Vertex(3), Hop::Shortcut),
            (Vertex(2), Vertex(4), Hop::Forward),
            (Vertex(3), Vertex(4), Hop::Shortcut),
        ],
        estimates: Vec::new(),
        goal: Vertex(4),
        pruned: Vec::new(),
    };
    for _ in 0..3 {
        let route = solved_actions(solve(
            &graph,
            generous_limits(),
            &AtomicBool::new(false),
            SearchMethod::AStar,
        ));
        assert_eq!(&*route, &[Hop::Forward, Hop::Forward]);
    }
}


/// Arbitrary successor counts have no hidden limit inherited from a card adapter.
#[test]
fn public_problem_can_generate_more_than_sixty_four_successors() {
    let graph = Graph {
        edges: (2..=130)
            .map(|next| (Vertex(1), Vertex(next), Hop::Forward))
            .collect(),
        estimates: Vec::new(),
        goal: Vertex(130),
        pruned: Vec::new(),
    };
    let result = solve(
        &graph,
        generous_limits(),
        &AtomicBool::new(false),
        SearchMethod::AStar,
    );
    match result {
        SearchOutcome::Solved { actions, stats } => {
            assert_eq!(&*actions, &[Hop::Forward]);
            assert_eq!(stats.generated, 129);
            assert_eq!(stats.states, 130);
        }
        other => panic!("expected the full successor iterator, received {other:?}"),
    }
}


/// Unrelated setting state owns no card identity, location or Pyramid inventory.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct SettingsState {
    /// Pending value awaiting application.
    value: u8,
    /// Whether the pending setting has been applied.
    applied: bool,
}


/// Logical operations in a minimal non-card settings workflow.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SettingsAction {
    /// Replace the pending value without applying it.
    SetValue(u8),
    /// Apply the pending setting.
    Apply,
}


/// Minimal public consumer relying on default zero heuristic and false pruning.
struct SettingsProblem;


impl SearchProblem for SettingsProblem {
    type State = SettingsState;
    type Action = SettingsAction;
    type Successors = std::option::IntoIter<(SettingsAction, SettingsState)>;

    /// Begin with the unchanged default setting.
    fn initial(&self) -> SettingsState {
        SettingsState {
            value: 0,
            applied: false,
        }
    }


    /// Require both the desired value and its explicit application.
    fn is_goal(&self, state: SettingsState) -> bool {
        state.value == 7 && state.applied
    }


    /// Yield one pending edit, followed by one independent apply operation.
    fn successors(&self, state: SettingsState) -> Self::Successors {
        let next = if state.value != 7 {
            Some((
                SettingsAction::SetValue(7),
                SettingsState {
                    value: 7,
                    applied: false,
                },
            ))
        } else if !state.applied {
            Some((
                SettingsAction::Apply,
                SettingsState {
                    value: 7,
                    applied: true,
                },
            ))
        } else {
            None
        };
        next.into_iter()
    }
}


/// Search-only integration accepts unrelated public state/action types unchanged.
#[test]
fn public_non_card_consumer_uses_default_guidance_and_typed_actions() {
    for method in [SearchMethod::AStar, SearchMethod::Dijkstra] {
        let result = solve(
            &SettingsProblem,
            generous_limits(),
            &AtomicBool::new(false),
            method,
        );
        match result {
            SearchOutcome::Solved { actions, stats } => {
                assert_eq!(
                    &*actions,
                    &[SettingsAction::SetValue(7), SettingsAction::Apply]
                );
                assert_eq!(stats.generated, 2);
                assert_eq!(stats.states, 3);
            }
            other => panic!("expected the settings workflow route, received {other:?}"),
        }
    }
}


/// Wrapper whose optional A* callbacks must never run in reference mode.
struct StrictReference;


impl SearchProblem for StrictReference {
    type State = SettingsState;
    type Action = SettingsAction;
    type Successors = std::option::IntoIter<(SettingsAction, SettingsState)>;

    /// Use the unrelated settings workflow's original state.
    fn initial(&self) -> SettingsState {
        SettingsProblem.initial()
    }


    /// Use the settings workflow's explicit application goal.
    fn is_goal(&self, state: SettingsState) -> bool {
        SettingsProblem.is_goal(state)
    }


    /// Preserve all settings transitions without consulting optional guidance.
    fn successors(&self, state: SettingsState) -> Self::Successors {
        SettingsProblem.successors(state)
    }


    /// Fail the test if reference search consults any heuristic.
    fn heuristic(&self, _state: SettingsState) -> u32 {
        panic!("Dijkstra must not invoke the problem heuristic")
    }


    /// Fail the test if reference search consults any pruning proof.
    fn impossible(&self, _state: SettingsState) -> bool {
        panic!("Dijkstra must not invoke the impossible-state proof")
    }
}


/// Dijkstra skips optional hooks entirely rather than discarding their results.
#[test]
fn reference_never_invokes_optional_guidance_callbacks() {
    assert!(matches!(
        solve(
            &StrictReference,
            generous_limits(),
            &AtomicBool::new(false),
            SearchMethod::Dijkstra,
        ),
        SearchOutcome::Solved { .. }
    ));
}


/// Iterator sets cancellation while yielding its first successor.
struct CancellingSuccessors {
    /// Shared search flag set during one bounded iterator step.
    cancel: Arc<AtomicBool>,
    /// Counts iterator steps to detect any work after cancellation.
    steps: Arc<AtomicUsize>,
    /// Whether the sole finite transition was already yielded.
    yielded: bool,
}


impl Iterator for CancellingSuccessors {
    type Item = (Hop, Vertex);

    /// Cancel during this step; the engine must stop before a second step.
    fn next(&mut self) -> Option<Self::Item> {
        self.steps.fetch_add(1, Ordering::Relaxed);
        if self.yielded {
            return None;
        }

        self.yielded = true;
        self.cancel.store(true, Ordering::Relaxed);
        Some((Hop::Forward, Vertex(2)))
    }
}


/// Problem used only to test cancellation at the iterator boundary.
struct CancellingProblem {
    /// Flag shared with the current solve invocation.
    cancel: Arc<AtomicBool>,
    /// Count shared with the assertion outside the solver.
    steps: Arc<AtomicUsize>,
}


impl SearchProblem for CancellingProblem {
    type State = Vertex;
    type Action = Hop;
    type Successors = CancellingSuccessors;

    /// Start before the only synthetic transition.
    fn initial(&self) -> Vertex {
        Vertex(1)
    }


    /// Mark the target as a goal that cancellation must prevent reaching.
    fn is_goal(&self, state: Vertex) -> bool {
        state == Vertex(2)
    }


    /// Return a lazily cancelling iterator rather than a prebuilt domain array.
    fn successors(&self, _state: Vertex) -> Self::Successors {
        CancellingSuccessors {
            cancel: Arc::clone(&self.cancel),
            steps: Arc::clone(&self.steps),
            yielded: false,
        }
    }
}


/// A flag set during successor iteration is observed before insertion or replay.
#[test]
fn cancellation_is_polled_between_individual_successor_steps() {
    let cancel = Arc::new(AtomicBool::new(false));
    let steps = Arc::new(AtomicUsize::new(0));
    let problem = CancellingProblem {
        cancel: Arc::clone(&cancel),
        steps: Arc::clone(&steps),
    };
    let result = solve(&problem, generous_limits(), &cancel, SearchMethod::AStar);
    assert!(matches!(
        result,
        SearchOutcome::Stopped {
            reason: SearchStop::Cancelled,
            stats: SearchStats { states: 1, .. },
        }
    ));
    assert_eq!(steps.load(Ordering::Relaxed), 1);
}


/// Large neutral states ensure accounting uses concrete generic storage sizes.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
struct LargeState([u8; 8192]);


/// Problem whose initial state alone exceeds a small memory budget.
struct LargeProblem;


impl SearchProblem for LargeProblem {
    type State = LargeState;
    type Action = ();
    type Successors = std::iter::Empty<((), LargeState)>;

    /// Return the oversized state without allocating a problem-owned collection.
    fn initial(&self) -> LargeState {
        LargeState([0; 8192])
    }


    /// Force initial insertion so its concrete storage cost must be checked.
    fn is_goal(&self, _state: LargeState) -> bool {
        false
    }


    /// No edges are needed to exercise the initial reservation boundary.
    fn successors(&self, _state: LargeState) -> Self::Successors {
        std::iter::empty()
    }
}


/// A large caller state cannot inherit the original small-state memory allowance.
#[test]
fn memory_limit_accounts_for_large_generic_state_before_allocation() {
    let mut limits = generous_limits();
    limits.max_memory_bytes = 4096;
    let result = solve(
        &LargeProblem,
        limits,
        &AtomicBool::new(false),
        SearchMethod::AStar,
    );
    assert!(matches!(
        result,
        SearchOutcome::Stopped {
            reason: SearchStop::MemoryLimit,
            stats: SearchStats { states: 0, .. },
        }
    ));
}


/// Large action payload tests generic output/history accounting independently.
struct LargeActionProblem;


impl SearchProblem for LargeActionProblem {
    type State = Vertex;
    type Action = [u8; 8192];
    type Successors = std::iter::Empty<([u8; 8192], Vertex)>;

    /// Return an ordinary small vertex key.
    fn initial(&self) -> Vertex {
        Vertex(1)
    }


    /// Force accounting for history and route action storage on initial insertion.
    fn is_goal(&self, _state: Vertex) -> bool {
        false
    }


    /// An initial reservation is sufficient to test concrete action sizing.
    fn successors(&self, _state: Vertex) -> Self::Successors {
        std::iter::empty()
    }
}


/// Large actions receive their concrete memory allowance even with tiny states.
#[test]
fn memory_limit_accounts_for_large_generic_action_before_allocation() {
    let mut limits = generous_limits();
    limits.max_memory_bytes = 4096;
    assert!(matches!(
        solve(
            &LargeActionProblem,
            limits,
            &AtomicBool::new(false),
            SearchMethod::AStar,
        ),
        SearchOutcome::Stopped {
            reason: SearchStop::MemoryLimit,
            stats: SearchStats { states: 0, .. },
        }
    ));
}


/// Workstation defaults and stable display messages remain source-compatible.
#[test]
fn existing_resource_defaults_and_stop_messages_are_preserved() {
    let limits = SearchLimits::default();
    assert_eq!(limits.max_states, 2_000_000);
    assert_eq!(limits.max_expansions, 2_000_000);
    assert_eq!(limits.max_memory_bytes, 512 * 1024 * 1024);
    assert_eq!(limits.max_duration, Duration::from_secs(120));
    for (reason, expected) in [
        (SearchStop::StateLimit, "state limit"),
        (SearchStop::ExpansionLimit, "expansion limit"),
        (SearchStop::MemoryLimit, "memory limit"),
        (SearchStop::TimeLimit, "time limit"),
        (SearchStop::Cancelled, "cancelled"),
        (SearchStop::AllocationFailure, "allocation failure"),
        (
            SearchStop::InternalInvariant,
            "internal search invariant failure",
        ),
    ] {
        assert_eq!(reason.to_string(), expected);
    }
}
