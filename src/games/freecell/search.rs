//! Unlimited guided Free Cell planning with independent physical replay.
//!
//! Guidance orders work, not a shortest-route promise. Exact symmetry keys retain
//! immutable physical histories. The additive fallback releases a maximum prefix
//! only after exhaustive absence of a complete route. No I/O or guest authority.

use super::{
    InitialDeal, replay,
    rules::{Position, RecordedMove, Successors},
};
use crate::search::{
    BestEffortProblem, PartialProof, SearchOutcome, SearchProblem, SearchProgress, SearchStats,
    SearchStop,
};
use std::{cell::Cell, sync::atomic::AtomicBool, time::Duration};


#[cfg(test)]
mod tests;


/// Actual counters and measured best banked-card count, never a percentage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Progress {
    /// Actual engine notification, including the final counters.
    pub search: SearchProgress,
    /// Largest foundation-card count actually encountered so far.
    pub best_foundation_cards: u32,
}


/// Complete outcome; interrupted/failed runs never expose a route prefix.
#[derive(Debug)]
pub enum Outcome {
    /// Every physical move independently replayed to all52 foundations.
    Solved {
        /// Ordered deliberate and separately tagged automatic transfers.
        moves: Box<[RecordedMove]>,
        /// Independently checked endpoint and cost counters.
        replay: replay::ReplaySummary,
        /// Actual final work.
        stats: SearchStats,
    },
    /// Entire generated graph exhausted without a reached goal.
    Exhausted(SearchStats),
    /// Cooperative cancellation, with no usable route.
    Cancelled(SearchStats),
    /// Actual allocation/arithmetic/invariant or independent replay failure.
    Failed {
        /// Typed failure, never a hidden configured search limit.
        failure: Failure,
        /// Final actual work.
        stats: SearchStats,
    },
}


/// Full-route preference, with a separately replayed exhaustive maximum fallback.
///
/// No interrupted or failed run supplies moves. A partial result proves maximum
/// foundation-card count under the supported rules, never challenge completion.
#[derive(Debug)]
pub enum BestEffortOutcome {
    /// Every physical move independently replayed to all52 foundations.
    Solved {
        /// Ordered deliberate and explicitly tagged automatic transfers.
        moves: Box<[RecordedMove]>,
        /// Independent complete-endpoint evidence and transfer counters.
        replay: replay::ReplaySummary,
        /// Actual final search work.
        stats: SearchStats,
    },
    /// Exhaustion proved no complete route and this maximum foundation endpoint.
    Partial {
        /// Independently replayed physical prefix, including automatic closure.
        moves: Box<[RecordedMove]>,
        /// Actual final search work, without a second graph traversal.
        stats: SearchStats,
        /// Independently counted deliberate cost-one transfers.
        deliberate_moves: usize,
        /// Independently counted automatic cost-zero transfers.
        automatic_moves: usize,
        /// Proved maximum banked-card count, 0..=51; zero grants no useful play.
        foundation_cards: u32,
    },
    /// Cooperative cancellation; no usable prefix.
    Cancelled(SearchStats),
    /// Actual engine or independent replay failure; no usable prefix.
    Failed {
        /// Original typed engine or replay failure.
        failure: Failure,
        /// Actual final search work.
        stats: SearchStats,
    },
}


/// Preserve engine and independently validated replay failure boundaries.
#[derive(Debug, thiserror::Error)]
pub enum Failure {
    /// Actual generic engine failure.
    #[error("Free Cell search failed: {0}")]
    Engine(SearchStop),
    /// Search output rejected by the separate replay implementation.
    #[error("{0}")]
    Replay(replay::ReplayError),
}


/// Immutable physical root and monotonic advisory progress.
struct Problem {
    /// Physical initial columns, never sorted in place.
    initial: Position,
    /// Largest actually encountered foundation count.
    best: Cell<u32>,
}


impl SearchProblem for Problem {
    type State = Position;
    type Action = RecordedMove;
    type Successors = Successors;


    /// Return the immutable physical starting position.
    fn initial(&self) -> Position {
        self.initial
    }


    /// Require all foundations complete and other piles empty.
    fn is_goal(&self, state: Position) -> bool {
        state.is_goal()
    }


    /// Generate finite moves without allocating transition log vectors.
    fn successors(&self, state: Position) -> Successors {
        state.successors()
    }


    /// Deliberate moves cost one; tagged automatic transfers cost zero.
    fn action_cost(&self, _state: Position, action: RecordedMove) -> u32 {
        u32::from(!action.is_auto)
    }
}


impl BestEffortProblem for Problem {
    /// Irreversibly banked cards, stable across exact canonical equality.
    fn score(&self, state: Position) -> u32 {
        state.foundation_cards()
    }
}


/// Find and independently replay a complete route, without configured cutoffs.
///
/// Original input is immutable. Checkpoints can suspend while retaining frontier;
/// observers/checkpoints must return promptly or deliberately pause. No guest I/O
/// is involved. Unlimited does not promise completion speed or OS OOM recovery.
///
/// # Panics
/// Caller observer/checkpoint panics propagate; system allocator termination is
/// outside recoverable Rust allocation errors.
pub fn solve_with_control(
    deal: &InitialDeal,
    cancel: &AtomicBool,
    progress: Option<&mut dyn FnMut(Progress)>,
    checkpoint: Option<&dyn Fn() -> Duration>,
) -> Outcome {
    let problem = Problem {
        initial: Position::initial(deal),
        best: Cell::new(0),
    };
    let guide = |state: Position| {
        problem
            .best
            .set(problem.best.get().max(state.foundation_cards()));
        state.guidance()
    };
    let mut progress = progress;
    let mut observe = |search: SearchProgress| {
        if let Some(observer) = progress.as_mut() {
            observer(Progress {
                search,
                best_foundation_cards: problem.best.get(),
            });
        }
    };
    match crate::search::solve_guided_unlimited_with_control(
        &problem,
        cancel,
        &guide,
        Some(&mut observe),
        checkpoint,
    ) {
        SearchOutcome::Solved { actions, stats } => {
            match replay::verify_with_checkpoint(deal, &actions, cancel, checkpoint) {
                Ok(summary) => Outcome::Solved {
                    moves: actions,
                    replay: summary,
                    stats,
                },
                Err(error) if error.kind == replay::ReplayFailure::Cancelled => {
                    Outcome::Cancelled(stats)
                }
                Err(error) => Outcome::Failed {
                    failure: Failure::Replay(error),
                    stats,
                },
            }
        }
        SearchOutcome::Unsolvable { stats } => Outcome::Exhausted(stats),
        SearchOutcome::Stopped {
            reason: SearchStop::Cancelled,
            stats,
        } => Outcome::Cancelled(stats),
        SearchOutcome::Stopped { reason, stats } => Outcome::Failed {
            failure: Failure::Engine(reason),
            stats,
        },
    }
}


/// Prefer a complete route, otherwise replay a proved maximum foundation prefix.
///
/// One uncapped guided traversal retains its best physical history. Only actual
/// graph exhaustion permits partial output; cancellation or any engine/replay
/// failure exposes no actions. A zero-card empty prefix is an honest no-benefit
/// result, not a playable plan. Complete routes make no shortest-route claim.
/// Original identities remain immutable, and no guest I/O is performed.
///
/// # Errors
/// Engine allocation/arithmetic/invariant or independent replay errors return
/// `Failed`; cooperative cancellation returns `Cancelled`. Neither has moves.
///
/// # Panics
/// Caller observer/checkpoint panics propagate; system allocator termination
/// remains outside recoverable Rust allocation errors.
pub fn solve_best_effort_with_control(
    deal: &InitialDeal,
    cancel: &AtomicBool,
    progress: Option<&mut dyn FnMut(Progress)>,
    checkpoint: Option<&dyn Fn() -> Duration>,
) -> BestEffortOutcome {
    let problem = Problem {
        initial: Position::initial(deal),
        best: Cell::new(0),
    };
    let guide = |state: Position| {
        problem
            .best
            .set(problem.best.get().max(state.foundation_cards()));
        state.guidance()
    };
    let mut progress = progress;
    let mut observe = |search| {
        if let Some(observer) = progress.as_mut() {
            observer(Progress {
                search,
                best_foundation_cards: problem.best.get(),
            });
        }
    };
    let outcome = crate::search::solve_guided_unlimited_best_effort_with_control(
        &problem,
        cancel,
        &guide,
        Some(&mut observe),
        checkpoint,
    );
    replay_outcome(deal, cancel, checkpoint, outcome)
}


/// Seal engine output only after independent complete or closed-prefix replay.
fn replay_outcome(
    deal: &InitialDeal,
    cancel: &AtomicBool,
    checkpoint: Option<&dyn Fn() -> Duration>,
    outcome: crate::search::BestEffortOutcome<RecordedMove>,
) -> BestEffortOutcome {
    match outcome {
        crate::search::BestEffortOutcome::Solved { actions, stats, .. } => {
            match replay::verify_with_checkpoint(deal, &actions, cancel, checkpoint) {
                Ok(replay) => BestEffortOutcome::Solved {
                    moves: actions.into_boxed_slice(),
                    replay,
                    stats,
                },
                Err(error) if error.kind == replay::ReplayFailure::Cancelled => {
                    BestEffortOutcome::Cancelled(stats)
                }
                Err(error) => BestEffortOutcome::Failed {
                    failure: Failure::Replay(error),
                    stats,
                },
            }
        }
        crate::search::BestEffortOutcome::Partial {
            actions,
            score,
            proof: PartialProof::Exhaustive,
            stats,
        } => match replay::verify_prefix_with_checkpoint(deal, &actions, cancel, checkpoint) {
            Ok(replay) => {
                let actual = replay
                    .endpoint
                    .foundations()
                    .iter()
                    .map(|n| u32::from(*n))
                    .sum::<u32>();
                if score >= 52 || actual != score {
                    return BestEffortOutcome::Failed {
                        failure: Failure::Engine(SearchStop::InternalInvariant),
                        stats,
                    };
                }
                BestEffortOutcome::Partial {
                    moves: actions.into_boxed_slice(),
                    stats,
                    deliberate_moves: replay.deliberate,
                    automatic_moves: replay.automatic,
                    foundation_cards: actual,
                }
            }
            Err(error) if error.kind == replay::ReplayFailure::Cancelled => {
                BestEffortOutcome::Cancelled(stats)
            }
            Err(error) => BestEffortOutcome::Failed {
                failure: Failure::Replay(error),
                stats,
            },
        },
        crate::search::BestEffortOutcome::Partial { stats, .. } => BestEffortOutcome::Failed {
            failure: Failure::Engine(SearchStop::InternalInvariant),
            stats,
        },
        crate::search::BestEffortOutcome::Stopped {
            reason: SearchStop::Cancelled,
            stats,
        } => BestEffortOutcome::Cancelled(stats),
        crate::search::BestEffortOutcome::Stopped { reason, stats } => BestEffortOutcome::Failed {
            failure: Failure::Engine(reason),
            stats,
        },
    }
}
