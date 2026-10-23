//! Independent validation of recorded Pyramid routes against the immutable deal.
//!
//! This validator deliberately uses explicit tableau presence and stock/waste
//! vectors rather than the solver's packed state, successor or transition code.
//! Original card indices identify cards without duplicating their identities.
//! A replay establishes legality under the agreed offline rules, not guest effect.

#![cfg_attr(
    not(test),
    deny(
        clippy::indexing_slicing,
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic
    )
)]

use super::CompleteDeal;

use super::planning::{
    Location, MAX_RECYCLES, MAX_SOLUTION_RECORDS, PYRAMID_MATCH_TOTAL, RecordedAction, ReplayError,
    ReplayStep, STOCK_LEN, TABLEAU_LEN, TABLEAU_ROWS,
};


/// Independently validate every action, optionally requiring all tableau cards gone.
///
/// Stock starts in encounter order with its first card accessible at DL; DR is
/// initially empty. MOVE transfers DL to DR, and RECYCLE turns a nonempty waste
/// back into stock only while stock is empty, at most twice. Both selections of
/// a MATCH must be accessible before either removal. Each trace row records
/// original indices 0..51 and before/after pile tops. Inputs remain unchanged.
///
/// # Errors
///
/// Returns `ReplayError` for an oversized route, illegal or unavailable selection,
/// invalid rank sum, forbidden draw/recycle, internal card-index inconsistency,
/// or a required final tableau clearance that was not reached.
/// Returns a one-based failing action in `ReplayError::step`, or zero for a
/// recording-length or missing-final-clear refusal. At most
/// `MAX_SOLUTION_RECORDS` trace rows are retained.
pub fn replay(
    deal: &CompleteDeal,
    actions: &[RecordedAction],
    require_clear: bool,
) -> Result<Box<[ReplayStep]>, ReplayError> {
    if actions.len() > MAX_SOLUTION_RECORDS {
        return Err(ReplayError {
            step: 0,
            reason: format!("route exceeds {MAX_SOLUTION_RECORDS} actions"),
        });
    }

    let mut tableau = [true; TABLEAU_LEN];
    let mut stock: Vec<u8> = (TABLEAU_LEN..TABLEAU_LEN + STOCK_LEN)
        .map(|index| index as u8)
        .collect();
    let mut waste: Vec<u8> = Vec::with_capacity(STOCK_LEN);
    let mut recycles = 0_u8;
    let mut trace = Vec::with_capacity(actions.len());

    for (ordinal, action) in actions.iter().copied().enumerate() {
        let step = ordinal + 1;
        let dl_before = stock.first().copied();
        let dr_before = waste.last().copied();
        let mut first = None;
        let mut second = None;
        let result: Result<(), String> = match action {
            RecordedAction::Match(left, right) => {
                resolve(left, &tableau, &stock, &waste).and_then(|left_id| {
                    resolve(right, &tableau, &stock, &waste).and_then(|right_id| {
                        if left_id == right_id {
                            return Err("MATCH requires two distinct cards".to_owned());
                        }
                        if value(deal, left_id)? + value(deal, right_id)? != PYRAMID_MATCH_TOTAL {
                            return Err("MATCH card values do not sum to 13".to_owned());
                        }

                        first = Some(left_id);
                        second = Some(right_id);
                        remove(left_id, &mut tableau, &mut stock, &mut waste)?;
                        remove(right_id, &mut tableau, &mut stock, &mut waste)?;
                        Ok(())
                    })
                })
            }
            RecordedAction::King(location) => {
                resolve(location, &tableau, &stock, &waste).and_then(|card_id| {
                    if value(deal, card_id)? != PYRAMID_MATCH_TOTAL {
                        return Err("KING requires a card with value 13".to_owned());
                    }

                    first = Some(card_id);
                    remove(card_id, &mut tableau, &mut stock, &mut waste)?;
                    Ok(())
                })
            }
            RecordedAction::Move => {
                if let Some(drawn) = stock.first().copied() {
                    stock.retain(|card_id| *card_id != drawn);
                    first = Some(drawn);
                    waste.push(drawn);
                    Ok(())
                } else {
                    Err("MOVE requires a nonempty DL pile; use RECYCLE for turnover".to_owned())
                }
            }
            RecordedAction::Recycle => {
                if !stock.is_empty() {
                    Err("RECYCLE requires an empty DL pile".to_owned())
                } else if waste.is_empty() {
                    Err("RECYCLE requires a nonempty DR pile".to_owned())
                } else if recycles >= MAX_RECYCLES {
                    Err("RECYCLE exceeds the two-recycle limit".to_owned())
                } else {
                    // Waste is bottom-to-top; stock is top-to-bottom. Turning
                    // over the physical pile therefore preserves vector order.
                    stock.append(&mut waste);
                    recycles += 1;
                    Ok(())
                }
            }
        };
        result.map_err(|reason| ReplayError { step, reason })?;

        trace.push(ReplayStep {
            action,
            first,
            second,
            dl_before,
            dr_before,
            dl_after: stock.first().copied(),
            dr_after: waste.last().copied(),
            remaining_tableau: tableau.iter().filter(|present| **present).count() as u8,
            recycles,
        });
    }

    if require_clear && tableau.iter().any(|present| *present) {
        return Err(ReplayError {
            step: 0,
            reason: "route ended before the tableau was cleared".to_owned(),
        });
    }

    Ok(trace.into_boxed_slice())
}


/// Resolve an accessible location before any mutation, retaining its original ID.
///
/// Private checked tableau locations guarantee array bounds; DL/DR select only
/// current visible tops. A covered or already removed card is refused.
fn resolve(
    location: Location,
    tableau: &[bool; TABLEAU_LEN],
    stock: &[u8],
    waste: &[u8],
) -> Result<u8, String> {
    match location {
        Location::DL => stock
            .first()
            .copied()
            .ok_or_else(|| "DL is empty".to_owned()),
        Location::DR => waste
            .last()
            .copied()
            .ok_or_else(|| "DR is empty".to_owned()),
        Location::Tableau(location) => {
            let index = location.index();
            let present = tableau
                .get(index)
                .ok_or_else(|| format!("tableau card index {index} is outside the board"))?;
            if !present {
                return Err(format!("tableau card {index} has already been removed"));
            }
            if !uncovered(tableau, index) {
                return Err(format!("tableau card {index} is covered"));
            }

            Ok(index as u8)
        }
    }
}


/// Check the two covering cells using independent bounded row-major arithmetic.
///
/// The caller supplies a valid tableau index. Bottom-row cards have no covering
/// cells; higher cells require both direct children to be absent.
fn uncovered(tableau: &[bool; TABLEAU_LEN], index: usize) -> bool {
    let mut row_start = 0;
    for row in 1..=TABLEAU_ROWS {
        let next_row = row_start + row;
        if index < next_row {
            if row == TABLEAU_ROWS {
                return true;
            }

            let column = index - row_start;
            return tableau
                .get(next_row + column)
                .is_some_and(|present| !present)
                && tableau
                    .get(next_row + column + 1)
                    .is_some_and(|present| !present);
        }
        row_start = next_row;
    }

    false
}


/// Read a rank value through checked access, refusing an invalid original index.
fn value(deal: &CompleteDeal, card_id: u8) -> Result<u8, String> {
    let index = usize::from(card_id);
    let card = if index < TABLEAU_LEN {
        deal.tableau().get(index)
    } else {
        deal.stock().get(index - TABLEAU_LEN)
    };
    card.map(|card| card.rank().value())
        .ok_or_else(|| format!("original card index {card_id} is outside the complete deal"))
}


/// Remove a previously resolved ID without changing any other original position.
///
/// MATCH resolves both selections first, so removing one pile top cannot change
/// which physical card the second selection denotes. IDs occur in only one pile.
/// Checked access refuses an invalid or absent ID rather than indexing a slice.
fn remove(
    card_id: u8,
    tableau: &mut [bool; TABLEAU_LEN],
    stock: &mut Vec<u8>,
    waste: &mut Vec<u8>,
) -> Result<(), String> {
    let index = usize::from(card_id);
    if index < TABLEAU_LEN {
        let present = tableau
            .get_mut(index)
            .filter(|present| **present)
            .ok_or_else(|| format!("tableau card index {card_id} is absent or invalid"))?;
        *present = false;
    } else if stock.contains(&card_id) {
        stock.retain(|candidate| *candidate != card_id);
    } else if waste.contains(&card_id) {
        waste.retain(|candidate| *candidate != card_id);
    } else {
        return Err(format!(
            "original card index {card_id} is absent from both piles"
        ));
    }

    Ok(())
}
