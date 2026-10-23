//! Complete-deal Pyramid rules for pure offline replay and route search.
//!
//! The immutable xSAR deal owns all card identities. A compact search state
//! stores only remaining-card bits, an original stock cursor and recycle count;
//! it cannot contain unknown occupants. Tableau positions are fixed. DL denotes
//! the first remaining stock card at or beyond the cursor, and DR the last
//! remaining card before it. Removing a card never changes original deck order.
//! Both visible pile tops are playable, with at most two waste recycles. The
//! goal is an empty 28-card tableau, regardless of cards left in the piles.
//!
//! Rules and the rank-count heuristic are adapted from Mitchell Chung's MIT
//! licensed `pyramid-solver`, revision
//! `c6dde73fd27ec817e2f1329f2a761b384cd8c663`. The repository retains the upstream
//! copyright and licence in `third_party/pyramid-solver-LICENSE`. These rules
//! describe the named offline rules profile;
//! they neither authorise guest input nor independently establish guest rules.

#![cfg_attr(
    not(test),
    deny(
        clippy::indexing_slicing,
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic
    )
)]

use crate::games::cards::Card;

use super::{
    CompleteDeal,
    planning::{
        DECK_LEN, FULL_DECK_MASK, Location, MAX_RECYCLES, MAX_SUCCESSORS, PYRAMID_MATCH_TOTAL,
        RecordedAction, RuleError, STOCK_LEN, SearchState, TABLEAU_LEN, TABLEAU_MASK, TABLEAU_ROWS,
        TableauLocation, tableau_coordinates, tableau_index,
    },
};


impl SearchState {
    /// Construct the complete initial board with stock ordinal zero visible in DL.
    ///
    /// All 52 original cards remain, DR is empty and no recycle has occurred.
    /// Only this constructor and checked rule transitions create public states.
    pub const fn initial() -> Self {
        Self {
            remaining: FULL_DECK_MASK,
            stock_cursor: 0,
            recycles: 0,
        }
    }


    /// Report whether every tableau card is removed, irrespective of pile cards.
    pub const fn is_goal(self) -> bool {
        self.remaining & TABLEAU_MASK == 0
    }


    /// Count remaining tableau cards without including either DRAW pile.
    pub const fn tableau_remaining(self) -> u32 {
        (self.remaining & TABLEAU_MASK).count_ones()
    }


    /// Test presence at an original checked tableau slot, including covered cards.
    ///
    /// This reports occupancy only. Use [`card_index`] to require current exposure
    /// before selecting a card; a present parent may still be covered.
    pub const fn tableau_present(self, location: TableauLocation) -> bool {
        self.remaining & (1_u64 << location.index()) != 0
    }


    /// Return the number of completed recycles, in `0..=MAX_RECYCLES`.
    pub const fn recycles(self) -> u8 {
        self.recycles
    }


    /// Return the original zero-based DL ordinal, or `STOCK_LEN` for empty stock.
    ///
    /// Checked transitions normalise this cursor past every removed stock card.
    /// DR is derived separately from the remaining cards before this ordinal.
    pub const fn stock_cursor(self) -> usize {
        self.stock_cursor as usize
    }
}


/// Read one original card by flat deck index, tableau first and then stock.
///
/// Indices `0..28` address the tableau and `28..52` the original stock order.
/// Invalid indices, including `u8::MAX`, return `None` before subtraction.
pub fn card_at(deal: &CompleteDeal, index: u8) -> Option<Card> {
    let index = usize::from(index);
    if index < TABLEAU_LEN {
        deal.tableau().get(index).copied()
    } else if index < DECK_LEN {
        deal.stock().get(index - TABLEAU_LEN).copied()
    } else {
        None
    }
}


/// Resolve a currently available fixed tableau location or visible pile top.
///
/// Tableau cards must remain and have both direct covering cards removed. DL is
/// the lowest remaining stock ordinal at or beyond the cursor; DR is the highest
/// remaining ordinal before it. Empty, removed and covered locations return
/// `None`. Returned indices always address the original 52-card deal.
pub fn card_index(state: SearchState, location: Location) -> Option<u8> {
    match location {
        Location::Tableau(location) => {
            let index = location.index();
            let index_byte = u8::try_from(index).ok()?;
            if !remains(state, index_byte) {
                return None;
            }
            let (row, column) = tableau_coordinates(index)?;
            if row < TABLEAU_ROWS {
                let left = u8::try_from(tableau_index(row + 1, column)?).ok()?;
                let right = u8::try_from(tableau_index(row + 1, column + 1)?).ok()?;
                if remains(state, left) || remains(state, right) {
                    return None;
                }
            }
            Some(index_byte)
        }
        Location::DL => (state.stock_cursor()..STOCK_LEN)
            .filter_map(stock_deck_index)
            .find(|&index| remains(state, index)),
        Location::DR => (0..state.stock_cursor().min(STOCK_LEN))
            .rev()
            .filter_map(stock_deck_index)
            .find(|&index| remains(state, index)),
    }
}


/// Apply one unit-cost operation under the fixed offline Pyramid rules profile.
///
/// A MATCH checks both locations against the original state before removing
/// either card, preserving simultaneous exposure and distinct-card requirements.
/// Kings remove one rank-13 card. MOVE advances DL to DR without removing cards.
/// RECYCLE requires empty DL, nonempty DR and fewer than two prior recycles.
/// Removal and drawing normalise DL to the next remaining original stock slot.
///
/// # Errors
///
/// Returns [`RuleError::Unavailable`] for empty, covered or removed locations or
/// an empty-stock MOVE; [`RuleError::InvalidLocation`] for two references to the
/// same card or an invalid original-card index; [`RuleError::WrongRank`] unless
/// a King is rank 13 or a pair totals 13; and the corresponding
/// [`RuleError::StockNotEmpty`], [`RuleError::EmptyWaste`] or
/// [`RuleError::RecycleLimit`] for a forbidden recycle. The input state and deal
/// remain unchanged on every error.
pub fn apply(
    deal: &CompleteDeal,
    state: SearchState,
    action: RecordedAction,
) -> Result<SearchState, RuleError> {
    let mut next = state;
    match action {
        RecordedAction::King(location) => {
            let index = card_index(state, location).ok_or(RuleError::Unavailable)?;
            let card = card_at(deal, index).ok_or(RuleError::InvalidLocation)?;
            if card.rank().value() != PYRAMID_MATCH_TOTAL {
                return Err(RuleError::WrongRank);
            }
            next.remaining &= !(1_u64 << index);
        }
        RecordedAction::Match(first, second) => {
            let first = card_index(state, first).ok_or(RuleError::Unavailable)?;
            let second = card_index(state, second).ok_or(RuleError::Unavailable)?;
            if first == second {
                return Err(RuleError::InvalidLocation);
            }
            let first_card = card_at(deal, first).ok_or(RuleError::InvalidLocation)?;
            let second_card = card_at(deal, second).ok_or(RuleError::InvalidLocation)?;
            if first_card.rank().value() + second_card.rank().value() != PYRAMID_MATCH_TOTAL {
                return Err(RuleError::WrongRank);
            }
            next.remaining &= !((1_u64 << first) | (1_u64 << second));
        }
        RecordedAction::Move => {
            let index = card_index(state, Location::DL).ok_or(RuleError::Unavailable)?;
            let ordinal = usize::from(index) - TABLEAU_LEN;
            next.stock_cursor =
                u8::try_from(ordinal + 1).map_err(|_| RuleError::InvalidLocation)?;
        }
        RecordedAction::Recycle => {
            if card_index(state, Location::DL).is_some() {
                return Err(RuleError::StockNotEmpty);
            }
            if card_index(state, Location::DR).is_none() {
                return Err(RuleError::EmptyWaste);
            }
            if state.recycles >= MAX_RECYCLES {
                return Err(RuleError::RecycleLimit);
            }
            next.stock_cursor = 0;
            next.recycles += 1;
        }
    }
    Ok(normalise_stock(next))
}


/// Generate every legal next action into fixed stack storage, without allocation.
///
/// Public states begin with a valid full tableau and only remove exposed cards.
/// Their exposed frontier contains at most seven tableau cards, plus two pile
/// tops. Even counting nine Kings, all 36 unordered pairs and one D-key action
/// gives at most 46 successors, within [`MAX_SUCCESSORS`]. Only one pair ordering
/// is generated because both orders have the same checked offline transition.
/// Unused entries are `None`; goal states have no generated successors.
pub fn successors(
    deal: &CompleteDeal,
    state: SearchState,
) -> [Option<(RecordedAction, SearchState)>; MAX_SUCCESSORS] {
    let mut output = [None; MAX_SUCCESSORS];
    if state.is_goal() {
        return output;
    }

    let mut available = [None; TABLEAU_ROWS + 2];
    let mut available_len = 0;
    for index in 0..TABLEAU_LEN {
        if let Some(location) = TableauLocation::from_index(index) {
            add_available(
                deal,
                state,
                Location::Tableau(location),
                &mut available,
                &mut available_len,
            );
        }
    }
    for location in [Location::DL, Location::DR] {
        add_available(deal, state, location, &mut available, &mut available_len);
    }

    let mut output_len = 0;
    for (offset, first) in available.iter().flatten().enumerate() {
        let (location, value) = *first;
        if value == PYRAMID_MATCH_TOTAL {
            add_successor(
                deal,
                state,
                RecordedAction::King(location),
                &mut output,
                &mut output_len,
            );
        }
        for &(second_location, second_value) in available.iter().flatten().skip(offset + 1) {
            if value + second_value == PYRAMID_MATCH_TOTAL {
                add_successor(
                    deal,
                    state,
                    RecordedAction::Match(location, second_location),
                    &mut output,
                    &mut output_len,
                );
            }
        }
    }
    let draw_action = if card_index(state, Location::DL).is_some() {
        RecordedAction::Move
    } else {
        RecordedAction::Recycle
    };
    add_successor(deal, state, draw_action, &mut output, &mut output_len);
    output
}


/// Bound remaining unit-cost actions using tableau rank counts only.
///
/// Each remaining King needs its own operation. For complementary rank pairs
/// `1/12` through `6/7`, at least the larger remaining tableau count is required:
/// one action can remove at most one card of either complementary rank. Drawing,
/// stock access and covering constraints are omitted, so this is an admissible
/// and consistent lower bound for the stated unit-cost rules.
pub fn heuristic(deal: &CompleteDeal, state: SearchState) -> u32 {
    let counts = rank_counts(deal, state, TABLEAU_LEN);
    let kings = counts
        .get(usize::from(PYRAMID_MATCH_TOTAL))
        .copied()
        .unwrap_or(0);
    let mut total = kings;
    for value in 1..=PYRAMID_MATCH_TOTAL / 2 {
        let first = counts.get(usize::from(value)).copied().unwrap_or(0);
        let second = counts
            .get(usize::from(PYRAMID_MATCH_TOTAL - value))
            .copied()
            .unwrap_or(0);
        total += first.max(second);
    }
    total
}


/// Conservatively prove impossibility when a tableau card has no usable complement.
///
/// A non-King requires a remaining complementary card that can be exposed at the
/// same time. A tableau ancestor or descendant cannot provide that match because
/// the descendant must be removed before the ancestor can be exposed. Stock
/// complements and unrelated tableau cards remain eligible regardless of current
/// access. `false` means this proof did not establish impossibility, not that a
/// solution exists. Search caches the deal-specific masks rather than rebuilding
/// them for every visited state.
pub fn impossible(deal: &CompleteDeal, state: SearchState) -> bool {
    impossible_with_masks(state, &complement_masks(deal))
}


/// Precompute eligible original complementary-card bits for each tableau slot.
///
/// Related tableau ancestors and descendants are excluded because they cannot
/// be exposed simultaneously. All complementary stock identities remain eligible
/// without claiming future pile access. A King's mask contains its own identity
/// so a remaining King alone never triggers this complementary-pair proof.
pub(crate) fn complement_masks(deal: &CompleteDeal) -> [u64; TABLEAU_LEN] {
    let mut masks = [0; TABLEAU_LEN];
    for (index, (card, mask)) in deal.tableau().iter().zip(masks.iter_mut()).enumerate() {
        if card.rank().value() == PYRAMID_MATCH_TOTAL {
            *mask = 1_u64 << index;
            continue;
        }
        for candidate in 0..DECK_LEN {
            let Some(candidate_byte) = u8::try_from(candidate).ok() else {
                continue;
            };
            let Some(other) = card_at(deal, candidate_byte) else {
                continue;
            };
            if card.rank().value() + other.rank().value() == PYRAMID_MATCH_TOTAL
                && (candidate >= TABLEAU_LEN || !related_tableau_positions(index, candidate))
            {
                *mask |= 1_u64 << candidate;
            }
        }
    }
    masks
}


/// Test cached complementary-card masks against current remaining identities.
///
/// Only remaining tableau positions require a match. Empty tableau positions
/// and all stock-only residual states pass this proof. Masks must be computed
/// from the immutable deal that owns this state's original identity ordering.
pub(crate) fn impossible_with_masks(state: SearchState, masks: &[u64; TABLEAU_LEN]) -> bool {
    masks.iter().enumerate().any(|(index, &mask)| {
        state.remaining & (1_u64 << index) != 0 && state.remaining & mask == 0
    })
}


/// Report a fixed tableau ancestor/descendant relation using checked coordinates.
///
/// Invalid indices return `false`. For distinct rows, a card at `(ra, ca)` covers
/// `(rb, cb)` exactly when `rb > ra` and `ca <= cb <= ca + rb - ra`. This includes
/// every depth, not just the two direct covering children.
fn related_tableau_positions(first: usize, second: usize) -> bool {
    let Some((first_row, first_column)) = tableau_coordinates(first) else {
        return false;
    };
    let Some((second_row, second_column)) = tableau_coordinates(second) else {
        return false;
    };
    if first_row == second_row {
        return first_column == second_column;
    }

    let ((ancestor_row, ancestor_column), (descendant_row, descendant_column)) =
        if first_row < second_row {
            ((first_row, first_column), (second_row, second_column))
        } else {
            ((second_row, second_column), (first_row, first_column))
        };
    ancestor_column <= descendant_column
        && descendant_column <= ancestor_column + (descendant_row - ancestor_row)
}


/// Test an original-card bit, rejecting out-of-deck inputs before shifting.
fn remains(state: SearchState, index: u8) -> bool {
    usize::from(index) < DECK_LEN && state.remaining & (1_u64 << index) != 0
}


/// Convert a valid original stock ordinal to its flat deck index without overflow.
fn stock_deck_index(ordinal: usize) -> Option<u8> {
    if ordinal >= STOCK_LEN {
        return None;
    }
    u8::try_from(TABLEAU_LEN + ordinal).ok()
}


/// Canonicalise DL after removals, draws or recycling without changing card bits.
fn normalise_stock(mut state: SearchState) -> SearchState {
    state.stock_cursor = card_index(state, Location::DL)
        .map(|index| index - TABLEAU_LEN as u8)
        .unwrap_or(STOCK_LEN as u8);
    state
}


/// Append one available location and rank within the proven nine-card frontier.
fn add_available(
    deal: &CompleteDeal,
    state: SearchState,
    location: Location,
    available: &mut [Option<(Location, u8)>; TABLEAU_ROWS + 2],
    length: &mut usize,
) {
    let Some(card) = card_index(state, location).and_then(|index| card_at(deal, index)) else {
        return;
    };
    // Checked public states preserve the seven-card frontier invariant, leaving
    // room for both pile tops. No valid branch can exceed this fixed capacity.
    if let Some(slot) = available.get_mut(*length) {
        *slot = Some((location, card.rank().value()));
        *length += 1;
    }
}


/// Append a legal operation within the documented 46-successor upper bound.
fn add_successor(
    deal: &CompleteDeal,
    state: SearchState,
    action: RecordedAction,
    output: &mut [Option<(RecordedAction, SearchState)>; MAX_SUCCESSORS],
    length: &mut usize,
) {
    if let Ok(next) = apply(deal, state, action) {
        // MAX_SUCCESSORS exceeds all nine singles, 36 pairs and one D-key action.
        // The checked write is therefore present for every reachable-state action.
        if let Some(slot) = output.get_mut(*length) {
            *slot = Some((action, next));
            *length += 1;
        }
    }
}


/// Count remaining rank values among the first bounded original-deck positions.
fn rank_counts(deal: &CompleteDeal, state: SearchState, limit: usize) -> [u32; 14] {
    let mut counts = [0; 14];
    for index in 0..limit.min(DECK_LEN) {
        let Some(index) = u8::try_from(index).ok() else {
            continue;
        };
        if remains(state, index)
            && let Some(card) = card_at(deal, index)
            && let Some(count) = counts.get_mut(usize::from(card.rank().value()))
        {
            *count += 1;
        }
    }
    counts
}


#[cfg(test)]
mod tests {
    use super::*;


    /// Load a complete ordered synthetic deck independently of live evidence.
    fn synthetic_deal() -> CompleteDeal {
        use crate::games::{
            cards::{Rank, Suit},
            pyramid::{DealMetadata, ProvenanceKind, Restoration},
        };

        let cards: [Card; DECK_LEN] =
            std::array::from_fn(|index| Card::new(Rank::ALL[index % 13], Suit::ALL[index / 13]));
        let metadata = DealMetadata::try_new(
            "synthetic-ordered-planning",
            ProvenanceKind::SyntheticFixture,
            Restoration::NotApplicable,
            "xSAR Pyramid rule tests",
            "Ordered standard deck constructed without JSON or live evidence",
            None,
        )
        .unwrap();
        CompleteDeal::try_new(
            cards[..TABLEAU_LEN].try_into().unwrap(),
            cards[TABLEAU_LEN..].try_into().unwrap(),
            metadata,
        )
        .unwrap()
    }


    /// Construct a checked test location by its original flat tableau index.
    fn tableau(index: usize) -> Location {
        Location::Tableau(TableauLocation::from_index(index).unwrap())
    }


    /// Build a small private test state from valid original-card indices.
    fn state_with(indices: &[u8], cursor: u8, recycles: u8) -> SearchState {
        SearchState {
            remaining: indices
                .iter()
                .fold(0, |bits, &index| bits | (1_u64 << index)),
            stock_cursor: cursor,
            recycles,
        }
    }


    /// Keep the full initial inventory, initial DL and empty DR explicit.
    #[test]
    fn initial_state_and_original_card_bounds() {
        let deal = synthetic_deal();
        let state = SearchState::initial();
        assert_eq!(state.tableau_remaining(), 28);
        assert_eq!(state.stock_cursor(), 0);
        assert_eq!(state.recycles(), 0);
        assert!(!state.is_goal());
        for index in 0..TABLEAU_LEN {
            assert!(state.tableau_present(TableauLocation::from_index(index).unwrap()));
        }
        assert_eq!(card_index(state, Location::DL), Some(28));
        assert_eq!(card_index(state, Location::DR), None);
        assert_eq!(card_index(state, tableau(0)), None);
        for index in 21..28 {
            assert_eq!(card_index(state, tableau(index)), Some(index as u8));
        }
        assert_eq!(card_at(&deal, 0), deal.tableau().first().copied());
        assert_eq!(card_at(&deal, 28), deal.stock().first().copied());
        assert_eq!(card_at(&deal, 51), deal.stock().last().copied());
        assert_eq!(card_at(&deal, 52), None);
        assert_eq!(card_at(&deal, u8::MAX), None);
    }


    /// A pair cannot first remove a covering card to expose its second selection.
    #[test]
    fn pairs_require_both_cards_available_in_the_original_state() {
        let deal = synthetic_deal();
        let state = state_with(&[15, 22], 24, 0);
        assert_eq!(card_at(&deal, 15).unwrap().rank().value(), 3);
        assert_eq!(card_at(&deal, 22).unwrap().rank().value(), 10);
        assert_eq!(card_index(state, tableau(15)), None);
        assert_eq!(card_index(state, tableau(22)), Some(22));
        assert_eq!(
            apply(
                &deal,
                state,
                RecordedAction::Match(tableau(22), tableau(15))
            ),
            Err(RuleError::Unavailable)
        );
        assert_eq!(state.remaining, (1_u64 << 15) | (1_u64 << 22));
    }


    /// Legal leaf pairs and Kings clear fixed bits without modifying the deal.
    #[test]
    fn removals_preserve_original_cards_and_refuse_invalid_ranks() {
        let deal = synthetic_deal();
        let original = deal.clone();
        let state = state_with(&[23, 25, 27], 24, 0);
        assert_eq!(
            apply(&deal, state, RecordedAction::King(tableau(23))),
            Err(RuleError::WrongRank)
        );
        assert_eq!(
            apply(
                &deal,
                state,
                RecordedAction::Match(tableau(23), tableau(23))
            ),
            Err(RuleError::InvalidLocation)
        );
        let state = apply(
            &deal,
            state,
            RecordedAction::Match(tableau(23), tableau(27)),
        )
        .unwrap();
        assert_eq!(state.tableau_remaining(), 1);
        assert_eq!(card_index(state, tableau(23)), None);
        assert!(!state.tableau_present(TableauLocation::from_index(23).unwrap()));
        assert!(state.tableau_present(TableauLocation::from_index(25).unwrap()));
        let state = apply(&deal, state, RecordedAction::King(tableau(25))).unwrap();
        assert!(state.is_goal());
        assert_eq!(deal, original);
    }


    /// Matching DL and DR removes only those cards and exposes older waste.
    #[test]
    fn both_pile_tops_are_playable_and_removals_normalise_dl() {
        let deal = synthetic_deal();
        let state = state_with(&[0, 28, 31, 32], 4, 0);
        assert_eq!(card_index(state, Location::DL), Some(32));
        assert_eq!(card_index(state, Location::DR), Some(31));
        let next = apply(
            &deal,
            state,
            RecordedAction::Match(Location::DR, Location::DL),
        )
        .unwrap();
        assert_eq!(next.stock_cursor(), 24);
        assert_eq!(card_index(next, Location::DL), None);
        assert_eq!(card_index(next, Location::DR), Some(28));
        assert_eq!(next.tableau_remaining(), 1);

        let state = state_with(&[0, 38, 39], 10, 0);
        let next = apply(&deal, state, RecordedAction::King(Location::DL)).unwrap();
        assert_eq!(next.stock_cursor(), 11);
        assert_eq!(card_index(next, Location::DL), Some(39));
        assert_eq!(card_index(next, Location::DR), None);
    }


    /// Drawing advances over removed original slots without deleting any card.
    #[test]
    fn draws_skip_gaps_and_keep_original_pile_order() {
        let deal = synthetic_deal();
        let mut state = state_with(&[0, 28, 32, 36], 0, 0);
        let original_bits = state.remaining;
        for (cursor, left, right) in [
            (4, Some(32), Some(28)),
            (8, Some(36), Some(32)),
            (24, None, Some(36)),
        ] {
            state = apply(&deal, state, RecordedAction::Move).unwrap();
            assert_eq!(state.stock_cursor(), cursor);
            assert_eq!(card_index(state, Location::DL), left);
            assert_eq!(card_index(state, Location::DR), right);
            assert_eq!(state.remaining, original_bits);
        }
        assert_eq!(
            apply(&deal, state, RecordedAction::Move),
            Err(RuleError::Unavailable)
        );
        state = apply(&deal, state, RecordedAction::Recycle).unwrap();
        assert_eq!(state.stock_cursor(), 0);
        assert_eq!(card_index(state, Location::DL), Some(28));
        assert_eq!(card_index(state, Location::DR), None);
        assert_eq!(state.remaining, original_bits);
    }


    /// Recycling requires nonempty waste, exhausted stock and a remaining budget.
    #[test]
    fn recycle_preconditions_and_two_recycle_limit_are_enforced() {
        let deal = synthetic_deal();
        let mut state = state_with(&[0, 28], 0, 0);
        assert_eq!(
            apply(&deal, state, RecordedAction::Recycle),
            Err(RuleError::StockNotEmpty)
        );
        for expected in 1..=MAX_RECYCLES {
            state = apply(&deal, state, RecordedAction::Move).unwrap();
            state = apply(&deal, state, RecordedAction::Recycle).unwrap();
            assert_eq!(state.recycles(), expected);
            assert_eq!(state.stock_cursor(), 0);
        }
        state = apply(&deal, state, RecordedAction::Move).unwrap();
        assert_eq!(
            apply(&deal, state, RecordedAction::Recycle),
            Err(RuleError::RecycleLimit)
        );
        assert_eq!(
            apply(&deal, state_with(&[0], 24, 0), RecordedAction::Recycle),
            Err(RuleError::EmptyWaste)
        );
    }


    /// Rank counts give a lower bound, and only missing complements prove failure.
    #[test]
    fn heuristic_and_conservative_impossibility_have_precise_meanings() {
        let deal = synthetic_deal();
        assert_eq!(heuristic(&deal, SearchState::initial()), 16);
        let state = state_with(&[23, 25, 27], 24, 0);
        assert_eq!(heuristic(&deal, state), 2);
        assert!(!impossible(&deal, state));
        assert!(impossible(&deal, state_with(&[0, 25], 24, 0)));
        assert!(!impossible(&deal, state_with(&[0, 25, 37], 9, 0)));
        assert!(!impossible(&deal, state_with(&[25], 24, 0)));
        assert!(!impossible(&deal, state_with(&[28], 0, 0)));
        assert_eq!(heuristic(&deal, state_with(&[28], 0, 0)), 0);
    }


    /// Exclude direct and transitive related cards from simultaneous-match masks.
    #[test]
    fn ancestor_only_complements_prove_the_tableau_cannot_clear() {
        let deal = synthetic_deal();
        let masks = complement_masks(&deal);

        // Three of diamonds at 6|1 is covered by ten of diamonds at 7|2.
        let state = state_with(&[15, 22], 24, 0);
        assert!(impossible(&deal, state));
        assert!(impossible_with_masks(state, &masks));
        assert_eq!(masks.get(15).unwrap() & (1_u64 << 22), 0);
        assert_eq!(masks.get(22).unwrap() & (1_u64 << 15), 0);

        // The apex Ace and both tableau Queens remain related at greater depth.
        let state = state_with(&[0, 11, 24], 24, 0);
        assert!(impossible_with_masks(state, &masks));
        assert_eq!(masks.first().unwrap() & ((1_u64 << 11) | (1_u64 << 24)), 0);
        assert!(related_tableau_positions(0, 24));
        assert!(related_tableau_positions(24, 0));
    }


    /// Preserve complementary cards outside the ancestor's descending footprint.
    #[test]
    fn unrelated_tableau_complements_remain_eligible() {
        let deal = synthetic_deal();
        let masks = complement_masks(&deal);
        // Four of diamonds at 6|2 and nine of diamonds at 7|1 are unrelated.
        let state = state_with(&[16, 21], 24, 0);
        assert!(!related_tableau_positions(16, 21));
        assert!(!related_tableau_positions(21, 16));
        assert!(!impossible_with_masks(state, &masks));
        assert_ne!(masks.get(16).unwrap() & (1_u64 << 21), 0);
        assert_ne!(masks.get(21).unwrap() & (1_u64 << 16), 0);
        assert!(
            apply(
                &deal,
                state,
                RecordedAction::Match(tableau(16), tableau(21))
            )
            .unwrap()
            .is_goal()
        );
        assert!(!related_tableau_positions(usize::MAX, 21));
        assert!(!related_tableau_positions(16, usize::MAX));
    }


    /// Retain every complementary stock identity independently of current access.
    #[test]
    fn stock_complements_prevent_false_impossibility_pruning() {
        let deal = synthetic_deal();
        let masks = complement_masks(&deal);
        // Ace of clubs remains with Queen of hearts at original stock ordinal 9.
        for cursor in [0, 9, 24] {
            let state = state_with(&[0, 37], cursor, 0);
            assert!(!impossible_with_masks(state, &masks));
            assert!(!impossible(&deal, state));
        }
        assert_ne!(masks.first().unwrap() & (1_u64 << 37), 0);
        assert!(impossible_with_masks(state_with(&[0], 24, 0), &masks));
    }


    /// A King supplies its own eligibility and never needs a complementary rank.
    #[test]
    fn kings_are_never_pruned_for_missing_complements() {
        let deal = synthetic_deal();
        let masks = complement_masks(&deal);
        for index in [12, 25] {
            assert_eq!(masks.get(index).copied(), Some(1_u64 << index));
            assert!(!impossible_with_masks(
                state_with(&[index as u8], 24, 0),
                &masks
            ));
        }
        assert!(!impossible_with_masks(state_with(&[], 24, 0), &masks));
    }


    /// Every generated branch is legal, unique and within the heuristic bound.
    #[test]
    fn fixed_successor_storage_has_complete_small_state_branches() {
        let deal = synthetic_deal();
        let state = state_with(&[23, 25, 27, 28], 0, 0);
        let successors = successors(&deal, state);
        let entries: Vec<_> = successors.into_iter().flatten().collect();
        assert_eq!(entries.len(), 3);
        assert!(
            entries
                .iter()
                .any(|&(action, _)| action == RecordedAction::King(tableau(25)))
        );
        assert!(
            entries
                .iter()
                .any(|&(action, _)| { action == RecordedAction::Match(tableau(23), tableau(27)) })
        );
        assert!(
            entries
                .iter()
                .any(|&(action, _)| action == RecordedAction::Move)
        );
        for (ordinal, &(action, next)) in entries.iter().enumerate() {
            assert_eq!(apply(&deal, state, action), Ok(next));
            assert!(heuristic(&deal, state) <= 1 + heuristic(&deal, next));
            assert!(
                !entries
                    .iter()
                    .skip(ordinal + 1)
                    .any(|&(_, other)| other == next)
            );
        }
        assert!(
            super::successors(&deal, state_with(&[28], 0, 0))
                .iter()
                .all(Option::is_none)
        );
    }
}
