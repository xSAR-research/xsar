//! Bounded deterministic A*, weighted A* and Dijkstra over nonnegative costs.
//!
//! This module owns search collections and immutable predecessor history. A problem
//! owns its domain state, legal transitions, goal and optional admissible heuristic
//! and sound pruning proof. No card, capture, transport or input policy is assumed.
//! Dijkstra invokes neither heuristic nor pruning, making it a reference search.
//!
//! Memory limits conservatively estimate retained collection capacities and route
//! output; they do not measure process RSS, allocator metadata, stack storage, or
//! problem-owned data and successor iterators. Every growing engine collection
//! uses fallible reservation. Problem callbacks must themselves terminate and keep
//! their work bounded because cancellation cannot interrupt a synchronous callback.

#![cfg_attr(
    not(test),
    deny(
        clippy::indexing_slicing,
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic
    )
)]

use crate::parameters::SEARCH_PROGRESS_INTERVAL as PROGRESS_INTERVAL;

use std::{
    cmp::Ordering,
    collections::{BinaryHeap, HashMap},
    hash::Hash,
    mem::size_of,
    num::NonZeroU32,
    sync::atomic::{AtomicBool, Ordering as AtomicOrdering},
    time::{Duration, Instant},
};


mod best_effort;


pub use best_effort::{BestEffortOutcome, BestEffortProblem, PartialProof, solve_best_effort};


#[cfg(test)]
mod cost_tests;


/// Explicit resource limits for one synchronous search.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SearchLimits {
    /// Maximum retained state records, including reopened paths.
    pub max_states: usize,
    /// Maximum expanded non-stale frontier entries.
    pub max_expansions: usize,
    /// Conservative collection-storage allowance in bytes, excluding process RSS.
    pub max_memory_bytes: usize,
    /// Cooperative wall-clock deadline measured from the start of search.
    pub max_duration: std::time::Duration,
}


impl Default for SearchLimits {
    /// Use bounded workstation defaults; callers may supply smaller/larger limits.
    fn default() -> Self {
        Self {
            max_states: 2_000_000,
            max_expansions: 2_000_000,
            max_memory_bytes: 512 * 1024 * 1024,
            max_duration: crate::parameters::DEFAULT_SEARCH_DURATION,
        }
    }
}


/// Search ordering; Dijkstra independently disables heuristic guidance/pruning.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchMethod {
    /// A* using the problem's admissible heuristic and sound impossible-state proof.
    AStar,
    /// Zero-heuristic reference minimum-cost search for manageable fixtures.
    Dijkstra,
}


/// Search ordering and positive heuristic weight for an optional guided search.
///
/// Weight one preserves exact A*. Larger weights favour the heuristic and may
/// return a higher-cost route. Dijkstra ignores this weight and all problem guidance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SearchOptions {
    /// A* or the independent zero-heuristic reference search.
    pub method: SearchMethod,
    /// Positive integer multiplier for A*'s admissible heuristic, not edge costs.
    pub heuristic_weight: NonZeroU32,
}


impl Default for SearchOptions {
    /// Preserve exact A* unless a caller explicitly requests a larger weight.
    fn default() -> Self {
        Self {
            method: SearchMethod::AStar,
            heuristic_weight: NonZeroU32::MIN,
        }
    }
}


/// Work actually completed by one bounded search.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SearchStats {
    /// States whose outgoing actions were examined.
    pub expanded: usize,
    /// Legal successor transitions generated, including previously seen states.
    pub generated: usize,
    /// Retained predecessor records, including any reopened paths.
    pub states: usize,
    /// Largest number of frontier entries retained at once.
    pub peak_frontier: usize,
    /// Elapsed synchronous search time, excluding input and report rendering.
    pub elapsed: std::time::Duration,
}


/// One bounded observer notification, without a queue or domain-specific policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SearchProgress {
    /// Actual cumulative work and elapsed search time at this notification.
    pub stats: SearchStats,
    /// True only for the single final notification before a normal return.
    pub finished: bool,
}


/// Reason search stopped without proving either a solution or impossibility.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchStop {
    /// Retained state/predecessor record limit reached.
    StateLimit,
    /// Expanded-state limit reached.
    ExpansionLimit,
    /// Conservative collection-memory allowance reached.
    MemoryLimit,
    /// Cooperative wall-clock deadline reached.
    TimeLimit,
    /// The caller's atomic cancellation flag was set.
    Cancelled,
    /// A checked collection allocation failed.
    AllocationFailure,
    /// A checked state/predecessor invariant or weighted-priority calculation failed.
    InternalInvariant,
}


impl std::fmt::Display for SearchStop {
    /// Retain stable human-readable stop messages without optional dependencies.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::StateLimit => "state limit",
            Self::ExpansionLimit => "expansion limit",
            Self::MemoryLimit => "memory limit",
            Self::TimeLimit => "time limit",
            Self::Cancelled => "cancelled",
            Self::AllocationFailure => "allocation failure",
            Self::InternalInvariant => "internal search invariant failure",
        })
    }
}


impl std::error::Error for SearchStop {}


/// Outcome preserving the distinction between proof and bounded interruption.
#[derive(Debug, Eq, PartialEq)]
pub enum SearchOutcome<A> {
    /// A goal-reaching path under the problem's declared transition rules.
    ///
    /// Dijkstra and weight-one A* establish minimum declared cost under the problem
    /// contract. A larger A* weight does not establish a minimum-cost route.
    Solved {
        /// Ordered actions from the initial state through the reached goal.
        actions: Box<[A]>,
        /// Actual work/time counters.
        stats: SearchStats,
    },
    /// Exhaustive search or sound impossible-state proof found no route.
    Unsolvable {
        /// Actual work/time counters.
        stats: SearchStats,
    },
    /// No claim of unsolvability or optimality follows from this interruption.
    Stopped {
        /// Specific bound/cancellation/allocation reason.
        reason: SearchStop,
        /// Actual work/time counters.
        stats: SearchStats,
    },
}


/// A pure nonnegative-cost problem independent of any domain or I/O backend.
///
/// Equal states must describe equivalent future transitions and goals. Successors
/// must yield all legal transitions in a stable order. Costs default to one;
/// an override must be deterministic for equal state/action pairs and may be zero.
/// Callbacks must terminate and successor iteration must be finite. The engine
/// checks cancellation and time between callbacks and individual successor steps,
/// but cannot interrupt a callback or account for problem-owned allocation.
///
/// For A* optimality, `heuristic` must use the same units as `action_cost`, never
/// exceed the remaining minimum cost, and
/// must return zero at goals. `impossible` may return true only when no goal can be
/// reached. The default zero heuristic and false proof are always conservative.
/// Dijkstra does not invoke either optional method.
pub trait SearchProblem {
    /// Compact state key including every detail that affects future transitions.
    type State: Copy + Eq + Hash;
    /// Typed operation used to reconstruct the complete returned route.
    type Action: Copy;
    /// Finite owned iterator over legal transitions in deterministic order.
    type Successors: Iterator<Item = (Self::Action, Self::State)>;

    /// Return the unchanged starting state for this invocation.
    fn initial(&self) -> Self::State;

    /// Whether the supplied state meets this problem's goal condition.
    fn is_goal(&self, state: Self::State) -> bool;

    /// Generate every legal edge without changing the problem.
    fn successors(&self, state: Self::State) -> Self::Successors;

    /// Return a stable nonnegative edge cost; zero-cost cycles are safely dominated.
    fn action_cost(&self, _state: Self::State, _action: Self::Action) -> u32 {
        1
    }

    /// Return an admissible lower bound in the declared action-cost units.
    fn heuristic(&self, _state: Self::State) -> u32 {
        0
    }

    /// Prove that no goal is reachable, or conservatively return false.
    fn impossible(&self, _state: Self::State) -> bool {
        false
    }
}


/// Conservative aggregate bytes allowed per immutable record before growth.
///
/// This caps retained records independently of unique states, including reopened
/// states and stale frontier entries. Capacity-based accounting below provides
/// a second guard and reserves room for the reconstructed action collection.
const MEMORY_BYTES_PER_RECORD: usize = 1024;


/// Smallest useful collection reservation, in elements rather than bytes.
const MIN_COLLECTION_CAPACITY: usize = 4;


/// Allowance for hash bucket rounding, control bytes and spare table capacity.
///
/// The standard map exposes usable element capacity rather than allocated bytes.
/// This deliberately generous factor is an estimate, not an allocator contract.
const HASH_MEMORY_FACTOR: usize = 4;


/// Borrowed synchronous observer and its most recent notification time.
///
/// This state is stack-owned and allocates no queue, thread or callback storage.
struct ProgressReporter<'a> {
    /// Optional caller-owned callback; it must return promptly and not panic.
    observer: Option<&'a mut dyn FnMut(SearchProgress)>,
    /// Search elapsed time at the last emitted notification, absent initially.
    last_reported: Option<Duration>,
}


impl<'a> ProgressReporter<'a> {
    /// Borrow a callback without invoking it or performing an allocation.
    fn new(observer: Option<&'a mut dyn FnMut(SearchProgress)>) -> Self {
        Self {
            observer,
            last_reported: None,
        }
    }


    /// Emit an initial, due periodic or unconditional final snapshot, if enabled.
    ///
    /// The final snapshot bypasses throttling so every normal outcome is visible.
    /// Callback time is cooperative work and cannot be interrupted by this module.
    /// Return true only when a callback ran, allowing an immediate STOP recheck.
    fn report(&mut self, stats: SearchStats, finished: bool) -> bool {
        let Some(observer) = self.observer.as_mut() else {
            return false;
        };
        if !finished
            && self
                .last_reported
                .is_some_and(|last| stats.elapsed.saturating_sub(last) < PROGRESS_INTERVAL)
        {
            return false;
        }

        self.last_reported = Some(stats.elapsed);
        observer(SearchProgress { stats, finished });
        true
    }
}


/// Discovered objective cost, with route length breaking equal-cost state ties.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
struct PathLabel {
    /// Sum of nonnegative edge costs, independent of reconstructed route length.
    cost: usize,
    /// Number of logical operations, including zero-cost transitions.
    depth: usize,
}


impl PathLabel {
    /// Extend both independent measures without wrapping either integer.
    fn successor(self, action_cost: u32) -> Result<Self, SearchStop> {
        let action_cost =
            usize::try_from(action_cost).map_err(|_| SearchStop::InternalInvariant)?;
        Ok(Self {
            cost: self
                .cost
                .checked_add(action_cost)
                .ok_or(SearchStop::InternalInvariant)?,
            depth: self
                .depth
                .checked_add(1)
                .ok_or(SearchStop::InternalInvariant)?,
        })
    }
}


/// One immutable history record, retained even after its state is reopened.
#[derive(Clone, Copy, Debug)]
struct Node<S, A> {
    /// Rule state reached by the operation stored in this record.
    state: S,
    /// Earlier arena record on this particular route, absent at the root.
    parent: Option<usize>,
    /// Operation from the parent, absent only for the initial state.
    action: Option<A>,
    /// Logical operations from the initial state, including zero-cost edges.
    depth: usize,
    /// Declared objective cost from the initial state to this record.
    cost: usize,
}


impl<S, A> Node<S, A> {
    /// Compare objective cost first without confusing it with history depth.
    fn label(&self) -> PathLabel {
        PathLabel {
            cost: self.cost,
            depth: self.depth,
        }
    }
}


/// Heap order: lowest f, highest cost, shortest equal-cost path, then serial.
#[derive(Clone, Copy, Debug)]
struct FrontierEntry {
    /// Immutable predecessor-arena index, also the stable insertion serial.
    node: usize,
    /// A* estimated total cost, or Dijkstra's known cost.
    priority: usize,
    /// Logical route length, used only after the objective-cost tie.
    depth: usize,
    /// Known objective cost; preserves the established unit-cost queue order.
    cost: usize,
}


impl PartialEq for FrontierEntry {
    /// Compare exactly the fields participating in the total heap ordering.
    fn eq(&self, other: &Self) -> bool {
        (self.priority, self.cost, self.depth, self.node)
            == (other.priority, other.cost, other.depth, other.node)
    }
}


impl Eq for FrontierEntry {}


impl Ord for FrontierEntry {
    /// Reverse f and insertion order because BinaryHeap pops the greatest item.
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .priority
            .cmp(&self.priority)
            .then_with(|| self.cost.cmp(&other.cost))
            .then_with(|| other.depth.cmp(&self.depth))
            .then_with(|| other.node.cmp(&self.node))
    }
}


impl PartialOrd for FrontierEntry {
    /// Delegate partial ordering to the deterministic total ordering.
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}


/// Owned bounded search collections and counters for one synchronous invocation.
struct Engine<'a, S, A> {
    /// Immutable state and predecessor records, including reopened histories.
    nodes: Vec<Node<S, A>>,
    /// Pending records; stale entries are skipped after a lower cost is found.
    frontier: BinaryHeap<FrontierEntry>,
    /// Lowest objective cost and then logical depth for each distinct rule state.
    best: HashMap<S, PathLabel>,
    /// Caller-supplied resource limits, unchanged throughout the invocation.
    limits: SearchLimits,
    /// Caller-owned flag, observed before work and between successors.
    cancel: &'a AtomicBool,
    /// Start of this invocation's elapsed-time budget.
    started: Instant,
    /// Accumulated counters, with elapsed time refreshed when reporting.
    stats: SearchStats,
}


impl<'a, S: Copy + Eq + Hash, A: Copy> Engine<'a, S, A> {
    /// Create empty collections without performing a heap allocation.
    fn new(limits: SearchLimits, cancel: &'a AtomicBool) -> Self {
        Self {
            nodes: Vec::new(),
            frontier: BinaryHeap::new(),
            best: HashMap::new(),
            limits,
            cancel,
            started: Instant::now(),
            stats: SearchStats {
                expanded: 0,
                generated: 0,
                states: 0,
                peak_frontier: 0,
                elapsed: std::time::Duration::ZERO,
            },
        }
    }


    /// Give cancellation precedence and stop once the elapsed budget is spent.
    fn poll(&self) -> Result<(), SearchStop> {
        if self.cancel.load(AtomicOrdering::Relaxed) {
            return Err(SearchStop::Cancelled);
        }

        if self.started.elapsed() >= self.limits.max_duration {
            return Err(SearchStop::TimeLimit);
        }

        Ok(())
    }


    /// Poll before and after an optional due observer without weakening STOP.
    fn poll_with_progress(&self, progress: &mut ProgressReporter<'_>) -> Result<(), SearchStop> {
        self.poll()?;
        if progress.observer.is_some() && progress.report(self.stats(), false) {
            self.poll()?;
        }

        Ok(())
    }


    /// Copy counters and measure elapsed time at the reported stopping point.
    fn stats(&self) -> SearchStats {
        SearchStats {
            expanded: self.stats.expanded,
            generated: self.stats.generated,
            states: self.nodes.len(),
            peak_frontier: self.stats.peak_frontier,
            elapsed: self.started.elapsed(),
        }
    }


    /// Report a budget, cancellation, allocation or invariant stop without proof.
    fn stopped(&self, reason: SearchStop) -> SearchOutcome<A> {
        SearchOutcome::Stopped {
            reason,
            stats: self.stats(),
        }
    }


    /// Reserve space for one history record, frontier entry and optional new key.
    ///
    /// Capacity growth is estimated before allocation and checked afterwards.
    /// A standard-library capacity rounding change may therefore stop conservatively
    /// after a reservation; the memory estimate is not a hard RSS or peak guarantee.
    fn reserve_insert(&mut self, new_state: bool) -> Result<(), SearchStop> {
        self.poll()?;
        if self.nodes.len() >= self.limits.max_states {
            return Err(SearchStop::StateLimit);
        }

        let record_cap = self.limits.max_memory_bytes / memory_bytes_per_record::<S, A>();
        if self.nodes.len() >= record_cap {
            return Err(SearchStop::MemoryLimit);
        }

        let node_target = growth_target(self.nodes.len(), self.nodes.capacity(), record_cap);
        let frontier_target =
            growth_target(self.frontier.len(), self.frontier.capacity(), record_cap);
        let map_target = if new_state {
            growth_target(self.best.len(), self.best.capacity(), record_cap)
        } else {
            self.best.capacity()
        };
        let map_estimate = if map_target > self.best.capacity() {
            map_target.saturating_mul(2).saturating_add(1)
        } else {
            self.best.capacity()
        };

        if estimated_bytes::<S, A>(node_target, frontier_target, map_estimate)
            > self.limits.max_memory_bytes
        {
            return Err(SearchStop::MemoryLimit);
        }

        if node_target > self.nodes.capacity() {
            self.nodes
                .try_reserve_exact(node_target.saturating_sub(self.nodes.len()))
                .map_err(|_| SearchStop::AllocationFailure)?;
        }

        if frontier_target > self.frontier.capacity() {
            self.frontier
                .try_reserve_exact(frontier_target.saturating_sub(self.frontier.len()))
                .map_err(|_| SearchStop::AllocationFailure)?;
        }

        if map_target > self.best.capacity() {
            self.best
                .try_reserve(map_target.saturating_sub(self.best.len()))
                .map_err(|_| SearchStop::AllocationFailure)?;
        }

        if estimated_bytes::<S, A>(
            self.nodes.capacity(),
            self.frontier.capacity(),
            self.best.capacity(),
        ) > self.limits.max_memory_bytes
        {
            return Err(SearchStop::MemoryLimit);
        }

        self.poll()
    }


    /// Insert a previously checked improvement without invalidating old parents.
    fn insert(
        &mut self,
        node: Node<S, A>,
        heuristic: u32,
        weight: NonZeroU32,
    ) -> Result<(), SearchStop> {
        self.poll()?;
        let priority = search_priority(node.cost, heuristic, weight)?;
        let new_state = !self.best.contains_key(&node.state);
        self.reserve_insert(new_state)?;
        let node_index = self.nodes.len();
        self.best.insert(node.state, node.label());
        self.nodes.push(node);
        self.frontier.push(FrontierEntry {
            node: node_index,
            priority,
            depth: node.depth,
            cost: node.cost,
        });
        self.stats.peak_frontier = self.stats.peak_frontier.max(self.frontier.len());
        Ok(())
    }


    /// Reconstruct a bounded route through immutable, strictly earlier records.
    ///
    /// Each parent must precede its child and have depth exactly one lower.
    /// Missing or inconsistent records produce InternalInvariant, never a route.
    fn route(
        &self,
        goal_index: usize,
        progress: &mut ProgressReporter<'_>,
    ) -> Result<Box<[A]>, SearchStop> {
        self.poll_with_progress(progress)?;
        let goal = self
            .nodes
            .get(goal_index)
            .ok_or(SearchStop::InternalInvariant)?;
        let mut actions = Vec::new();
        actions
            .try_reserve_exact(goal.depth)
            .map_err(|_| SearchStop::AllocationFailure)?;
        let extra_action_capacity = actions.capacity().saturating_sub(self.nodes.capacity());
        let allocated_bytes = estimated_bytes::<S, A>(
            self.nodes.capacity(),
            self.frontier.capacity(),
            self.best.capacity(),
        )
        .saturating_add(extra_action_capacity.saturating_mul(size_of::<A>()));
        if allocated_bytes > self.limits.max_memory_bytes {
            return Err(SearchStop::MemoryLimit);
        }

        let mut index = goal_index;
        let mut depth = goal.depth;
        loop {
            self.poll_with_progress(progress)?;
            let node = self.nodes.get(index).ok_or(SearchStop::InternalInvariant)?;
            if node.depth != depth {
                return Err(SearchStop::InternalInvariant);
            }

            match (node.parent, node.action) {
                (Some(parent), Some(action)) if parent < index && depth > 0 => {
                    actions.push(action);
                    index = parent;
                    depth -= 1;
                }
                (None, None) if depth == 0 => break,
                _ => return Err(SearchStop::InternalInvariant),
            }
        }

        if actions.len() != goal.depth {
            return Err(SearchStop::InternalInvariant);
        }

        actions.reverse();
        Ok(actions.into_boxed_slice())
    }
}


/// Reject unrepresentable objective priorities, including exact weight-one keys.
fn search_priority(cost: usize, heuristic: u32, weight: NonZeroU32) -> Result<usize, SearchStop> {
    let heuristic = usize::try_from(heuristic).map_err(|_| SearchStop::InternalInvariant)?;
    let weight = usize::try_from(weight.get()).map_err(|_| SearchStop::InternalInvariant)?;
    heuristic
        .checked_mul(weight)
        .and_then(|weighted| cost.checked_add(weighted))
        .ok_or(SearchStop::InternalInvariant)
}


/// Compute geometric growth bounded by the conservative retained-record cap.
fn growth_target(length: usize, capacity: usize, maximum: usize) -> usize {
    if length < capacity {
        capacity
    } else {
        capacity
            .saturating_mul(2)
            .max(MIN_COLLECTION_CAPACITY)
            .min(maximum)
    }
}


/// Estimate allocated collections plus enough actions for the longest arena path.
///
/// Saturating arithmetic treats an unrepresentable estimate as maximally large.
fn estimated_bytes<S, A>(nodes: usize, frontier: usize, states: usize) -> usize {
    nodes
        .saturating_mul(size_of::<Node<S, A>>().saturating_add(size_of::<A>()))
        .saturating_add(frontier.saturating_mul(size_of::<FrontierEntry>()))
        .saturating_add(
            states
                .saturating_mul(size_of::<(S, PathLabel)>().saturating_add(1))
                .saturating_mul(HASH_MEMORY_FACTOR),
        )
}


/// Bound records by at least 1,024 bytes and by the concrete generic storage.
///
/// Large caller state/action types increase the cap divisor rather than inheriting
/// the small-state allowance. Saturation treats unrepresentable sizes as maximal.
fn memory_bytes_per_record<S, A>() -> usize {
    estimated_bytes::<S, A>(1, 1, 1).max(MEMORY_BYTES_PER_RECORD)
}


/// Find a minimum declared-cost route through a pure caller-owned problem.
///
/// A* relies on the problem's documented admissibility and sound-pruning contract.
/// Dijkstra ignores both methods. Cancellation precedes zero resource budgets and
/// is checked before allocation, each successor step and route reconstruction.
/// Resource limits, allocation failure and cancellation return `Stopped` with no
/// partial route; only exhausted search or a sound pruning proof is `Unsolvable`.
///
/// Memory accounting includes concrete state/action sizes, immutable history,
/// stale heap entries, hash capacity and output capacity, with at least 1,024
/// bytes allowed per retained record. This is a conservative collection estimate,
/// not an operating-system RSS or peak-allocation guarantee. Problem-owned storage
/// and callback work remain the caller's responsibility. This function performs
/// no I/O or background work and does not mutate the problem.
pub fn solve<P: SearchProblem>(
    problem: &P,
    limits: SearchLimits,
    cancel: &AtomicBool,
    method: SearchMethod,
) -> SearchOutcome<P::Action> {
    solve_with_options(
        problem,
        limits,
        cancel,
        SearchOptions {
            method,
            heuristic_weight: NonZeroU32::MIN,
        },
        None,
    )
}


/// Find a bounded declared-cost route with optional weighted A* and progress reports.
///
/// A* orders the frontier by `g + heuristic_weight * h` and returns the first
/// non-stale popped goal. Weight one retains [`solve`]'s minimum-cost guarantee;
/// larger weights do not establish minimum cost. Dijkstra ignores the weight,
/// heuristic and pruning. Reopening, predecessor checks and all supplied limits
/// apply to every mode. A larger weight does not guarantee a faster search.
///
/// An observer receives an initial snapshot, periodic snapshots at most once per
/// 200 ms, and exactly one final snapshot matching the returned outcome's stats.
/// The final snapshot bypasses throttling and may follow an initial or periodic
/// snapshot immediately. Reports occur at cooperative polling boundaries, so a
/// long callback, allocation or problem operation can delay them. The observer
/// runs synchronously, must return promptly, and owns any reporting side effects.
/// No observer storage, queue, thread or background work is allocated here.
///
/// Observer time before completion counts against the deadline. Cancellation is
/// polled after every non-final callback. The final notification observes an
/// already determined outcome; its duration and changes to cancellation are not
/// included in that outcome. No further search work follows it.
///
/// # Errors
/// Resource bounds, cancellation and failed reservations return `Stopped` without
/// a partial route. Weighted-priority overflow returns `InternalInvariant` before
/// inserting a record. Only exhausted search or sound pruning is `Unsolvable`.
/// Memory accounting and problem contracts are the same as [`solve`].
///
/// # Panics
/// A panic from a caller's problem or observer propagates; the final-notification
/// guarantee applies to normal returns, not unwinding or process termination.
pub fn solve_with_options<P: SearchProblem>(
    problem: &P,
    limits: SearchLimits,
    cancel: &AtomicBool,
    options: SearchOptions,
    progress: Option<&mut dyn FnMut(SearchProgress)>,
) -> SearchOutcome<P::Action> {
    let mut engine = Engine::<P::State, P::Action>::new(limits, cancel);
    let mut progress = ProgressReporter::new(progress);
    if progress.observer.is_some() {
        progress.report(engine.stats(), false);
    }

    let outcome = run(problem, &mut engine, options, &mut progress);
    let stats = match &outcome {
        SearchOutcome::Solved { stats, .. }
        | SearchOutcome::Unsolvable { stats }
        | SearchOutcome::Stopped { stats, .. } => *stats,
    };
    progress.report(stats, true);
    outcome
}


/// Run one exact or weighted search, leaving final notification to the caller.
fn run<P: SearchProblem>(
    problem: &P,
    engine: &mut Engine<'_, P::State, P::Action>,
    options: SearchOptions,
    progress: &mut ProgressReporter<'_>,
) -> SearchOutcome<P::Action> {
    if let Err(reason) = engine.poll_with_progress(progress) {
        return engine.stopped(reason);
    }

    let initial = problem.initial();
    if let Err(reason) = engine.poll_with_progress(progress) {
        return engine.stopped(reason);
    }

    let use_astar = matches!(options.method, SearchMethod::AStar);
    let weight = if use_astar {
        options.heuristic_weight
    } else {
        NonZeroU32::MIN
    };
    let initial_is_goal = problem.is_goal(initial);
    if let Err(reason) = engine.poll_with_progress(progress) {
        return engine.stopped(reason);
    }

    if initial_is_goal {
        return SearchOutcome::Solved {
            actions: Box::new([]),
            stats: engine.stats(),
        };
    }

    let initial_is_impossible = use_astar && problem.impossible(initial);
    if let Err(reason) = engine.poll_with_progress(progress) {
        return engine.stopped(reason);
    }

    if initial_is_impossible {
        return SearchOutcome::Unsolvable {
            stats: engine.stats(),
        };
    }

    let heuristic = if use_astar {
        problem.heuristic(initial)
    } else {
        0
    };
    if let Err(reason) = engine.insert(
        Node {
            state: initial,
            parent: None,
            action: None,
            depth: 0,
            cost: 0,
        },
        heuristic,
        weight,
    ) {
        return engine.stopped(reason);
    }

    loop {
        if let Err(reason) = engine.poll_with_progress(progress) {
            return engine.stopped(reason);
        }

        let Some(entry) = engine.frontier.pop() else {
            return SearchOutcome::Unsolvable {
                stats: engine.stats(),
            };
        };
        let Some(node) = engine.nodes.get(entry.node).copied() else {
            return engine.stopped(SearchStop::InternalInvariant);
        };
        if engine.best.get(&node.state) != Some(&node.label()) {
            continue;
        }

        let is_goal = problem.is_goal(node.state);
        if let Err(reason) = engine.poll_with_progress(progress) {
            return engine.stopped(reason);
        }

        if is_goal {
            return match engine.route(entry.node, progress) {
                Ok(actions) => SearchOutcome::Solved {
                    actions,
                    stats: engine.stats(),
                },
                Err(reason) => engine.stopped(reason),
            };
        }

        if engine.stats.expanded >= engine.limits.max_expansions {
            return engine.stopped(SearchStop::ExpansionLimit);
        }

        engine.stats.expanded += 1;
        let mut successors = problem.successors(node.state);
        loop {
            if let Err(reason) = engine.poll_with_progress(progress) {
                return engine.stopped(reason);
            }

            let Some((action, next)) = successors.next() else {
                break;
            };
            if let Err(reason) = engine.poll_with_progress(progress) {
                return engine.stopped(reason);
            }

            engine.stats.generated = engine.stats.generated.saturating_add(1);
            if use_astar && problem.impossible(next) {
                continue;
            }

            if let Err(reason) = engine.poll_with_progress(progress) {
                return engine.stopped(reason);
            }
            let action_cost = problem.action_cost(node.state, action);
            if let Err(reason) = engine.poll_with_progress(progress) {
                return engine.stopped(reason);
            }
            let label = match node.label().successor(action_cost) {
                Ok(label) => label,
                Err(reason) => return engine.stopped(reason),
            };
            if engine.best.get(&next).is_some_and(|known| *known <= label) {
                continue;
            }

            let heuristic = if use_astar {
                problem.heuristic(next)
            } else {
                0
            };
            if let Err(reason) = engine.insert(
                Node {
                    state: next,
                    parent: Some(entry.node),
                    action: Some(action),
                    depth: label.depth,
                    cost: label.cost,
                },
                heuristic,
                weight,
            ) {
                return engine.stopped(reason);
            }
        }
    }
}


#[cfg(test)]
mod options_tests {
    //! Deterministic clock-gate and arithmetic checks without waiting on a timer.

    use super::*;


    /// Periodic reports respect 200 ms while a matching final report bypasses it.
    #[test]
    fn progress_throttle_uses_elapsed_time_and_always_emits_final() {
        let mut snapshots = Vec::new();
        let mut observer = |snapshot| snapshots.push(snapshot);
        let mut progress = ProgressReporter::new(Some(&mut observer));
        for millis in [0, 1, 199, 200, 399, 400, 401] {
            progress.report(
                SearchStats {
                    elapsed: Duration::from_millis(millis),
                    ..SearchStats::default()
                },
                false,
            );
        }

        let final_stats = SearchStats {
            expanded: 9,
            elapsed: Duration::from_millis(401),
            ..SearchStats::default()
        };
        progress.report(final_stats, true);
        assert_eq!(snapshots.len(), 4);
        let elapsed: Vec<_> = snapshots
            .iter()
            .map(|snapshot| snapshot.stats.elapsed.as_millis())
            .collect();
        assert_eq!(elapsed, [0, 200, 400, 401]);
        assert_eq!(snapshots.last().unwrap().stats, final_stats);
        assert_eq!(
            snapshots
                .iter()
                .filter(|snapshot| snapshot.finished)
                .count(),
            1
        );
    }


    /// A due periodic observer can cancel before another unit of engine work.
    #[test]
    fn cancellation_is_polled_immediately_after_a_periodic_observer() {
        let cancel = AtomicBool::new(false);
        let mut engine = Engine::<u8, u8>::new(SearchLimits::default(), &cancel);
        engine.started = Instant::now().checked_sub(PROGRESS_INTERVAL).unwrap();
        let mut callbacks = 0;
        let mut observer = |_snapshot| {
            callbacks += 1;
            cancel.store(true, AtomicOrdering::Relaxed);
        };
        let mut progress = ProgressReporter::new(Some(&mut observer));
        progress.last_reported = Some(Duration::ZERO);
        assert_eq!(
            engine.poll_with_progress(&mut progress),
            Err(SearchStop::Cancelled)
        );
        assert_eq!(callbacks, 1);
        assert!(engine.nodes.is_empty());
        assert!(engine.frontier.is_empty());
        assert!(engine.best.is_empty());
    }


    /// Weighted keys either preserve their exact integer value or stop explicitly.
    #[test]
    fn weighted_priority_rejects_overflow_including_weight_one() {
        for (depth, heuristic, weight) in [(5, 7, 2), (usize::MAX, 1, 2), (0, u32::MAX, u32::MAX)] {
            let exact = depth as u128 + u128::from(heuristic) * u128::from(weight);
            let result = search_priority(depth, heuristic, NonZeroU32::new(weight).unwrap());
            if exact > usize::MAX as u128 {
                assert_eq!(result, Err(SearchStop::InternalInvariant));
            } else {
                assert_eq!(result, Ok(exact as usize));
            }
        }

        assert_eq!(
            search_priority(usize::MAX, u32::MAX, NonZeroU32::MIN),
            Err(SearchStop::InternalInvariant),
        );
    }
}
