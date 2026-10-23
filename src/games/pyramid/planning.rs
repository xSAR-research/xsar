//! Checked locations, actions and compact states for the fixed Pyramid rules.
//!
//! The immutable complete deal owns all card identities. Planning stores original
//! indices and presence bits only; one-based row/column mappings never renumber
//! positions after removal. These pure types neither perform I/O nor grant input
//! authority. The independently replayed trace remains separate from search state.

#![cfg_attr(
    not(test),
    deny(
        clippy::indexing_slicing,
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic
    )
)]

use std::fmt;

use thiserror::Error;

pub use super::{STOCK_LEN, TABLEAU_LEN};


/// Number of rows in a standard 28-card Pyramid tableau.
pub const TABLEAU_ROWS: usize = 7;


/// Maximum actions accepted by the bounded independent route validator.
///
/// This resource bound is separate from game legality and permits routes longer
/// than the retained 48- and 49-action acceptance examples.
pub const MAX_SOLUTION_RECORDS: usize = 4096;


/// A checked tableau location stored as one flat index in `0..TABLEAU_LEN`.
///
/// The private byte retains the 28-position domain without storing redundant
/// row and column integers. External recording coordinates remain one-based.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TableauLocation {
    /// Validated flat index in the original row-major tableau array.
    index: u8,
}


impl TableauLocation {
    /// Construct a location from a flat index, rejecting indices outside 0..28.
    pub fn from_index(index: usize) -> Option<Self> {
        if index >= TABLEAU_LEN {
            return None;
        }

        u8::try_from(index).ok().map(|index| Self { index })
    }


    /// Return the original row-major array index without changing its identity.
    pub const fn index(self) -> usize {
        self.index as usize
    }
}


/// A fixed tableau position or one of the two currently visible DRAW piles.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Location {
    /// A position whose one-based row and column are validated before storage.
    Tableau(TableauLocation),
    /// The currently visible DRAW pile on the left of the tableau.
    DL,
    /// The currently visible DRAW pile on the right of the tableau.
    DR,
}


impl fmt::Display for Location {
    /// Print the CSV's one-based `row|column`, `DL`, or `DR` location notation.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Tableau(_) => {
                let (row, column) = row_column_from_location(*self).ok_or(fmt::Error)?;
                write!(formatter, "{row}|{column}")
            }
            Self::DL => formatter.write_str("DL"),
            Self::DR => formatter.write_str("DR"),
        }
    }
}


/// A recorded operation, retaining card selection order for two-card matches.
///
/// These values describe the manual recording; they do not establish that an
/// operation is legal against a changing stock, waste, or tableau state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecordedAction {
    /// Select the first card and then the second card for a sum-of-13 match.
    Match(Location, Location),
    /// Remove the King at the recorded location with a single selection.
    King(Location),
    /// Press the D key to advance the DRAW piles.
    Move,
    /// Press the D key to recycle; kept distinct for recording clarity.
    Recycle,
}


impl RecordedAction {
    /// Return the canonical uppercase action label used in the CSV.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Match(_, _) => "MATCH",
            Self::King(_) => "KING",
            Self::Move => "MOVE",
            Self::Recycle => "RECYCLE",
        }
    }


    /// Return CARD1 and CARD2 without reordering the recorded selections.
    pub const fn locations(self) -> (Option<Location>, Option<Location>) {
        match self {
            Self::Match(first, second) => (Some(first), Some(second)),
            Self::King(location) => (Some(location), None),
            Self::Move | Self::Recycle => (None, None),
        }
    }


    /// Return a human-readable description without claiming route legality.
    pub const fn operation(self) -> &'static str {
        match self {
            Self::Match(_, _) => "Match CARD1 then CARD2 (sum 13)",
            Self::King(_) => "Remove the King at CARD1",
            Self::Move => "Press D",
            Self::Recycle => "Press D to recycle",
        }
    }
}


/// Number of original card identities retained by the complete deal.
pub const DECK_LEN: usize = TABLEAU_LEN + STOCK_LEN;


/// Presence mask for all 52 original card identities.
pub const FULL_DECK_MASK: u64 = (1_u64 << DECK_LEN) - 1;


/// Presence mask for the 28 original tableau identities.
pub const TABLEAU_MASK: u64 = (1_u64 << TABLEAU_LEN) - 1;


/// Recycles permitted by the pinned upstream rules: three stock passes total.
pub const MAX_RECYCLES: u8 = 2;


/// Numeric total required to remove a pair, or the value of a removable King.
pub const PYRAMID_MATCH_TOTAL: u8 = 13;


/// Stack capacity exceeding nine visible cards' 36 pairs, nine Kings and one D.
pub const MAX_SUCCESSORS: usize = 64;


/// Canonical complete-knowledge search position; card identities remain in xSAR.
///
/// Bits 0..28 describe tableau presence and 28..52 original stock presence.
/// The stock cursor separates waste ordinals below it from undrawn ordinals.
/// Fields are internal so public callers cannot construct invalid cursors/masks.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SearchState {
    /// Presence of each original card; bits above 51 are always zero.
    pub(crate) remaining: u64,
    /// First remaining undrawn stock ordinal, or 24 for exhausted stock.
    pub(crate) stock_cursor: u8,
    /// Completed waste-to-stock recycles, in 0..=2.
    pub(crate) recycles: u8,
}


/// A refused pure transition; its supplied state remains unchanged.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RuleError {
    /// A location cannot identify a card in this state.
    #[error("invalid card location")]
    InvalidLocation,
    /// A card is absent, covered, or selected twice.
    #[error("card unavailable, covered, or selected twice")]
    Unavailable,
    /// A King is not rank 13 or a pair does not total 13.
    #[error("King must be 13 and a pair must total 13")]
    WrongRank,
    /// Recycle was requested while undrawn stock remains.
    #[error("cannot recycle while stock remains")]
    StockNotEmpty,
    /// There is no waste to recycle or no stock to draw.
    #[error("requested DRAW pile is empty")]
    EmptyWaste,
    /// Two recycles have already been consumed.
    #[error("the two-recycle limit has been reached")]
    RecycleLimit,
}


/// One independently replayed action with original identity indices for printing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReplayStep {
    /// Action retaining the exact ordered tableau/pile locations.
    pub action: RecordedAction,
    /// First selected card, or the drawn stock card, as original identity 0..52.
    pub first: Option<u8>,
    /// Second selected card for a pair, otherwise absent.
    pub second: Option<u8>,
    /// Visible left top before the action.
    pub dl_before: Option<u8>,
    /// Visible right top before the action.
    pub dr_before: Option<u8>,
    /// Visible left top after the action.
    pub dl_after: Option<u8>,
    /// Visible right top after the action.
    pub dr_after: Option<u8>,
    /// Number of occupied tableau slots after the action.
    pub remaining_tableau: u8,
    /// Recycles used after the action.
    pub recycles: u8,
}


/// Independent replay refusal with the first failing one-based action number.
#[derive(Debug, Error)]
#[error("route step {step}: {reason}")]
pub struct ReplayError {
    /// One-based action number, or zero for an invalid initial/final contract.
    pub step: usize,
    /// Specific failure from the independent model.
    pub reason: String,
}


/// Shared bounded-search resources and outcomes used by the Pyramid adapter.
#[cfg(feature = "pyramid_search")]
pub use crate::search::{
    SearchLimits, SearchMethod, SearchOptions, SearchProgress, SearchStats, SearchStop,
};


/// Bounded search outcome containing typed Pyramid actions on a solved route.
#[cfg(feature = "pyramid_search")]
pub type SearchOutcome = crate::search::SearchOutcome<RecordedAction>;


/// Convert `1 <= column <= row <= 7` to its row-major index in `0..28`.
///
/// Invalid coordinates return `None` before multiplication or subtraction, so
/// even `usize::MAX` inputs cannot overflow or underflow the mapping formula.
pub fn tableau_index(row: usize, column: usize) -> Option<usize> {
    if !(1..=TABLEAU_ROWS).contains(&row) || !(1..=row).contains(&column) {
        return None;
    }

    Some(row * (row - 1) / 2 + (column - 1))
}


/// Convert a flat array index in `0..28` to its one-based row and column.
///
/// Returns `None` for every index outside the complete tableau. The bounded
/// seven-row scan uses integer arithmetic without floating-point rounding.
pub fn tableau_coordinates(index: usize) -> Option<(usize, usize)> {
    if index >= TABLEAU_LEN {
        return None;
    }

    let mut row_start = 0;
    for row in 1..=TABLEAU_ROWS {
        if index < row_start + row {
            return Some((row, index - row_start + 1));
        }
        row_start += row;
    }

    None
}


/// Construct a fixed tableau location from valid one-based row/column inputs.
///
/// Returns `None` unless `1 <= column <= row <= 7`; DRAW pile markers are never
/// produced by this coordinate-only conversion.
pub fn location_from_row_column(row: usize, column: usize) -> Option<Location> {
    tableau_index(row, column)
        .and_then(TableauLocation::from_index)
        .map(Location::Tableau)
}


/// Return a tableau location's one-based coordinates, or `None` for DL and DR.
pub fn row_column_from_location(location: Location) -> Option<(usize, usize)> {
    match location {
        Location::Tableau(location) => tableau_coordinates(location.index()),
        Location::DL | Location::DR => None,
    }
}


#[cfg(test)]
mod tests {
    use super::*;


    /// Keep the checked flat location closed over exactly the 28 original slots.
    #[test]
    fn tableau_location_indices_are_checked() {
        for index in 0..TABLEAU_LEN {
            assert_eq!(
                TableauLocation::from_index(index).map(TableauLocation::index),
                Some(index)
            );
        }

        assert_eq!(TableauLocation::from_index(TABLEAU_LEN), None);
        assert_eq!(TableauLocation::from_index(usize::MAX), None);
    }


    /// Exhaustively prove all 28 one-based coordinates round-trip in row order.
    #[test]
    fn every_tableau_coordinate_round_trips() {
        let mut expected_index = 0;
        let mut occurrences = [0; TABLEAU_LEN];
        for row in 1..=TABLEAU_ROWS {
            for column in 1..=row {
                let index = tableau_index(row, column).unwrap();
                assert_eq!(index, expected_index);
                *occurrences.get_mut(index).unwrap() += 1;
                assert_eq!(tableau_coordinates(expected_index), Some((row, column)));
                let location = location_from_row_column(row, column).unwrap();
                assert_eq!(row_column_from_location(location), Some((row, column)));
                expected_index += 1;
            }
        }

        assert_eq!(expected_index, TABLEAU_LEN);
        assert_eq!(occurrences, [1; TABLEAU_LEN]);
    }


    /// Reject invalid bounds before mapping arithmetic, including extreme inputs.
    #[test]
    fn invalid_and_extreme_coordinates_are_rejected() {
        for (row, column) in [
            (0, 0),
            (0, 1),
            (1, 0),
            (3, 4),
            (7, 8),
            (8, 1),
            (usize::MAX, 1),
            (1, usize::MAX),
            (usize::MAX, usize::MAX),
        ] {
            assert_eq!(tableau_index(row, column), None);
            assert_eq!(location_from_row_column(row, column), None);
        }

        assert_eq!(tableau_coordinates(TABLEAU_LEN), None);
        assert_eq!(tableau_coordinates(TABLEAU_LEN + 1), None);
        assert_eq!(tableau_coordinates(usize::MAX), None);
    }


    /// Pile locations cannot be mistaken for tableau coordinates.
    #[test]
    fn piles_have_no_tableau_coordinates() {
        assert_eq!(row_column_from_location(Location::DL), None);
        assert_eq!(row_column_from_location(Location::DR), None);
    }
}
