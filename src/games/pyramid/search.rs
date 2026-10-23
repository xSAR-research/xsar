//! Pyramid's fixed rules adapter with explicit, independently chosen cost units.
//!
//! The adapter borrows the complete deal and caches eligible complementary-card
//! masks. It owns no cards, performs no I/O and grants no guest input authority.
//! The generic engine owns allocation, search order, cancellation and resource
//! bounds. Independent replay remains a separate model of explicit DRAW piles.

#![cfg_attr(
    not(test),
    deny(
        clippy::indexing_slicing,
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic
    )
)]

use std::sync::atomic::AtomicBool;

use crate::search::{BestEffortOutcome, BestEffortProblem, SearchProblem};

use super::{
    CompleteDeal,
    planning::{
        MAX_SUCCESSORS, RecordedAction, SearchLimits, SearchMethod, SearchOptions, SearchOutcome,
        SearchProgress, SearchState, TABLEAU_LEN,
    },
    rules,
};


/// Objective-cost policy; legacy solver entry points retain logical-action cost.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PyramidCost {
    /// Every MATCH, KING, MOVE and RECYCLE costs one, preserving existing APIs.
    LogicalActions,
    /// MATCH and KING cost one; MOVE and RECYCLE have zero match cost.
    ///
    /// Stock-only removals still cost one but contribute no tableau-removal score.
    /// A minimum-cost proof concerns removal operations, not total input count.
    CardMatches,
}


impl PyramidCost {
    /// Keep pair, King and pile-only operations explicit without counting progress.
    fn action_cost(self, action: RecordedAction) -> u32 {
        match (self, action) {
            (Self::CardMatches, RecordedAction::Move | RecordedAction::Recycle) => 0,
            _ => 1,
        }
    }
}


/// Rule adapter borrowing the sole immutable source of card identities.
struct PyramidGraph<'a> {
    /// Validated original deal, shared by all states without cloning its cards.
    deal: &'a CompleteDeal,
    /// Eligible complements for each original tableau card, excluding relatives.
    complement_masks: [u64; TABLEAU_LEN],
    /// Explicit declared units, never inferred from the requested heuristic weight.
    cost: PyramidCost,
}


impl SearchProblem for PyramidGraph<'_> {
    /// Compact complete-knowledge position without card identity duplication.
    type State = SearchState;
    /// One logical Pyramid operation, preserving selection order and zero-cost draws.
    type Action = RecordedAction;
    /// Stable stack-backed successor order, omitting unused storage slots.
    type Successors = std::iter::Flatten<
        std::array::IntoIter<Option<(RecordedAction, SearchState)>, MAX_SUCCESSORS>,
    >;


    /// Begin with all 52 cards present, the first stock card at DL and DR empty.
    fn initial(&self) -> SearchState {
        SearchState::initial()
    }


    /// Tableau clearance is the agreed board-level goal, regardless of pile cards.
    fn is_goal(&self, state: SearchState) -> bool {
        state.is_goal()
    }


    /// Delegate every legal next operation to the checked fixed Pyramid rules.
    fn successors(&self, state: SearchState) -> Self::Successors {
        rules::successors(self.deal, state).into_iter().flatten()
    }


    /// Apply the declared cost policy without changing rules or scoring stock cards.
    fn action_cost(&self, _state: SearchState, action: RecordedAction) -> u32 {
        self.cost.action_cost(action)
    }


    /// Bound remaining removals using the complementary tableau rank counts.
    fn heuristic(&self, state: SearchState) -> u32 {
        rules::heuristic(self.deal, state)
    }


    /// Use the conservative proof that a remaining tableau card lacks a partner.
    fn impossible(&self, state: SearchState) -> bool {
        rules::impossible_with_masks(state, &self.complement_masks)
    }
}


impl BestEffortProblem for PyramidGraph<'_> {
    /// Count removed original tableau cards only; stock removals supply no score.
    fn score(&self, state: SearchState) -> u32 {
        TABLEAU_LEN as u32 - (state.remaining & super::planning::TABLEAU_MASK).count_ones()
    }
}


/// Seek a complete route, falling back to the most tableau cards found removable.
///
/// All logical operations retain unit cost. Full-clear search honours the
/// requested weight; fallback disables impossible-goal pruning and uses only
/// the remaining aggregate limits. `PartialProof::Exhaustive` proves maximum
/// tableau removals and shortest cost at that score; `Bounded` is best-found
/// only. Stock removals do not increase the score. Cancellation, allocation or
/// invariant failure exposes no route. Callers must independently replay every
/// returned route, allowing a non-clear endpoint only for a partial outcome.
/// No I/O or live-game authority is granted. Existing solver APIs are unchanged.
///
/// # Panics
/// Caller observer panics propagate as documented by the generic engine.
pub fn solve_best_effort_with_options(
    deal: &CompleteDeal,
    limits: SearchLimits,
    cancel: &AtomicBool,
    options: SearchOptions,
    progress: Option<&mut dyn FnMut(SearchProgress)>,
) -> BestEffortOutcome<RecordedAction> {
    solve_best_effort_with_cost(
        deal,
        limits,
        cancel,
        options,
        PyramidCost::LogicalActions,
        progress,
    )
}


/// Seek a full route, then maximum tableau removal, in explicit objective units.
///
/// `CardMatches` counts every MATCH and single KING as one removal action,
/// including stock-only removals; DRAW and RECYCLE cost zero. Only original
/// tableau cards increase fallback score. The tableau rank-count heuristic is
/// admissible in either policy because each paid removal reduces it by at most
/// one and free pile advances leave it unchanged. Existing entry points retain
/// `LogicalActions`. Exact A* or Dijkstra proves minimum declared cost only;
/// equal-cost total action count is not an additional A* optimality guarantee.
///
/// Both phases share the original aggregate limits, cancellation and deadline.
/// A bounded partial result is best-found, whereas exhaustive fallback proves
/// maximum tableau removal and least cost at that score. Callers must replay
/// the complete route, including every zero-cost pile operation, independently.
/// No capture, live acceptance or input authority follows from this result.
///
/// # Panics
/// Caller observer panics propagate as documented by the generic engine.
pub fn solve_best_effort_with_cost(
    deal: &CompleteDeal,
    limits: SearchLimits,
    cancel: &AtomicBool,
    options: SearchOptions,
    cost: PyramidCost,
    progress: Option<&mut dyn FnMut(SearchProgress)>,
) -> BestEffortOutcome<RecordedAction> {
    crate::search::solve_best_effort(
        &PyramidGraph {
            deal,
            complement_masks: rules::complement_masks(deal),
            cost,
        },
        limits,
        cancel,
        options,
        progress,
    )
}


/// Find a minimum logical-operation route from a complete deal's initial state.
///
/// Every MATCH, KING, MOVE and RECYCLE costs one. A* uses the agreed admissible
/// heuristic and safe pruning; Dijkstra disables both to serve as a reference.
/// The synchronous generic engine checks cancellation before allocation, between
/// expansions and successors, and during reconstruction. The caller may set the
/// atomic flag from any thread. The deal remains unchanged.
///
/// A solved result is optimal for this rules profile. Unsolvable means the
/// eligible frontier was exhausted or a sound impossible-state proof succeeded.
/// Resource cutoffs, cancellation and allocation failure return Stopped without
/// a partial route or a claim that the deal is unsolvable. The memory bound is a
/// conservative collection-storage estimate, not measured process RSS. No I/O,
/// guest input, background work or live-rules verification occurs here.
pub fn solve(
    deal: &CompleteDeal,
    limits: SearchLimits,
    cancel: &AtomicBool,
    method: SearchMethod,
) -> SearchOutcome {
    crate::search::solve(
        &PyramidGraph {
            deal,
            complement_masks: rules::complement_masks(deal),
            cost: PyramidCost::LogicalActions,
        },
        limits,
        cancel,
        method,
    )
}


/// Find a bounded route with optional weighted A* and synchronous progress.
///
/// Every MATCH, KING, MOVE and RECYCLE retains unit cost. Weight one establishes
/// minimum action count; larger A* weights may return a longer route. Dijkstra
/// ignores the weight and all heuristic/pruning guidance. The complete deal and
/// fixed rules remain unchanged, and callers must independently replay a solved
/// route before using it. A larger weight does not guarantee a faster search.
///
/// The borrowed observer receives initial, at most five periodic notifications
/// per second, and one final snapshot matching the returned outcome's stats.
/// The final notification bypasses throttling. Callbacks must return promptly;
/// cancellation cannot interrupt them. Search and non-final observer time count
/// against the supplied deadline. No I/O, queue, thread or guest authority is
/// introduced by this adapter. See [`crate::search::solve_with_options`] for the
/// callback, cancellation, memory-accounting and final-notification contracts.
///
/// # Errors
/// Resource bounds, cancellation, allocation failure and weighted-key overflow
/// produce `Stopped`, never a partial route or an unsolvability claim.
///
/// # Panics
/// A caller observer panic propagates and prevents normal final notification.
pub fn solve_with_options(
    deal: &CompleteDeal,
    limits: SearchLimits,
    cancel: &AtomicBool,
    options: SearchOptions,
    progress: Option<&mut dyn FnMut(SearchProgress)>,
) -> SearchOutcome {
    crate::search::solve_with_options(
        &PyramidGraph {
            deal,
            complement_masks: rules::complement_masks(deal),
            cost: PyramidCost::LogicalActions,
        },
        limits,
        cancel,
        options,
        progress,
    )
}


#[cfg(test)]
mod cost_tests {
    //! Explicit operation units remain separate from tableau-progress scoring.

    use super::*;
    use crate::games::pyramid::planning::{Location, TableauLocation};


    /// Pile-only matches and single Kings cost one even though they add no tableau score.
    #[test]
    fn match_policy_counts_pairs_and_kings_but_not_draw_or_recycle() {
        let tableau = Location::Tableau(TableauLocation::from_index(27).unwrap());
        for action in [
            RecordedAction::Match(Location::DL, Location::DR),
            RecordedAction::Match(tableau, Location::DL),
            RecordedAction::King(Location::DR),
            RecordedAction::King(tableau),
        ] {
            assert_eq!(PyramidCost::CardMatches.action_cost(action), 1);
            assert_eq!(PyramidCost::LogicalActions.action_cost(action), 1);
        }
        for action in [RecordedAction::Move, RecordedAction::Recycle] {
            assert_eq!(PyramidCost::CardMatches.action_cost(action), 0);
            assert_eq!(PyramidCost::LogicalActions.action_cost(action), 1);
        }
    }
}
