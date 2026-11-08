//! Exact-key, physical mutation and independently enumerated small-graph checks.

use super::*;
use std::collections::{HashMap, VecDeque};
use std::hash::DefaultHasher;


/// Build a complete near-goal physical position with validated cached hashes.
fn position(columns: [&[u8]; 8], cells: [u8; 4], foundations: [u8; 4]) -> Position {
    let mut p = Position {
        cards: [EMPTY; 52],
        lengths: [0; 8],
        cells,
        foundations,
        column_hashes: [0; 8],
    };
    let mut offset = 0;
    for (c, cards) in columns.iter().enumerate() {
        p.lengths[c] = cards.len() as u8;
        p.cards[offset..offset + cards.len()].copy_from_slice(cards);
        offset += cards.len();
    }
    for c in 0..8 {
        p.refresh(c);
    }
    p.validate().unwrap();
    p
}


/// Hash as the production dictionary does, independently of exact equality.
fn digest(p: Position) -> u64 {
    let mut h = DefaultHasher::new();
    p.hash(&mut h);
    h.finish()
}


/// True hash collisions cannot collapse order/length; physical permutations remain exact symmetries.
#[test]
fn freecell_310_exact_keys_cache_collisions_and_physical_symmetry() {
    let p = position(
        [
            &[11, 12],
            &[24, 25],
            &[37, 38],
            &[50, 51],
            &[],
            &[],
            &[],
            &[],
        ],
        [EMPTY; 4],
        [11; 4],
    );
    let q = position(
        [
            &[50, 51],
            &[],
            &[24, 25],
            &[11, 12],
            &[],
            &[37, 38],
            &[],
            &[],
        ],
        [EMPTY; 4],
        [11; 4],
    );
    assert_eq!(p, q);
    assert_eq!(digest(p), digest(q));
    assert_ne!(p.column(0), q.column(0));
    let mut reordered = position(
        [
            &[12, 11],
            &[24, 25],
            &[37, 38],
            &[50, 51],
            &[],
            &[],
            &[],
            &[],
        ],
        [EMPTY; 4],
        [11; 4],
    );
    let regrouped = position(
        [
            &[11],
            &[12, 24, 25],
            &[37, 38],
            &[50, 51],
            &[],
            &[],
            &[],
            &[],
        ],
        [EMPTY; 4],
        [11; 4],
    );
    assert_ne!(p, reordered);
    assert_ne!(p, regrouped);
    let mut forced_collision = p;
    forced_collision.column_hashes = [0; 8];
    reordered.column_hashes = [0; 8];
    assert_eq!(digest(forced_collision), digest(reordered));
    let mut map = HashMap::new();
    map.insert(forced_collision, 1);
    map.insert(reordered, 2);
    assert_eq!(map.len(), 2);
    let p = position(
        [
            &[10, 11],
            &[23, 24],
            &[36, 37],
            &[49, 50],
            &[],
            &[],
            &[],
            &[],
        ],
        [12, 25, 38, 51],
        [10; 4],
    );
    let q = position(
        [
            &[49, 50],
            &[36, 37],
            &[23, 24],
            &[10, 11],
            &[],
            &[],
            &[],
            &[],
        ],
        [51, 38, 25, 12],
        [10; 4],
    );
    assert_eq!(p, q);
    assert_eq!(digest(p), digest(q));
}


/// Both packed movement directions preserve untouched columns and exact cache contents.
#[test]
fn freecell_310_single_card_moves_are_checked_and_foundations_irreversible() {
    for reversed in [false, true] {
        let columns: [&[u8]; 8] = if reversed {
            [
                &[23, 24, 25],
                &[10, 11],
                &[36, 37, 38],
                &[49, 50, 51],
                &[],
                &[],
                &[],
                &[],
            ]
        } else {
            [
                &[10, 11],
                &[23, 24, 25],
                &[36, 37, 38],
                &[49, 50, 51],
                &[],
                &[],
                &[],
                &[],
            ]
        };
        let p = position(columns, [12, EMPTY, EMPTY, EMPTY], [10; 4]);
        assert!(p.automatic().is_none());
        let (from, to) = if reversed { (1, 0) } else { (0, 1) };
        let next = p
            .apply(RecordedMove {
                card_id: 11,
                from: Pile::Cascade(from),
                to: Pile::Cascade(to),
                is_auto: false,
            })
            .unwrap();
        next.validate().unwrap();
        assert_eq!(next.column(2), p.column(2));
        assert_eq!(next.column(3), p.column(3));
        assert_eq!(next.column(usize::from(from)).unwrap(), &[10]);
        assert_eq!(next.column(usize::from(to)).unwrap(), &[23, 24, 25, 11]);
        assert!(
            p.successors()
                .any(|(a, _)| a.from == Pile::Cell(0) && a.to == Pile::Cascade(4))
        );
        assert!(
            !p.successors()
                .any(|(a, _)| a.from == Pile::Cell(0) && matches!(a.to, Pile::Cascade(5..=7)))
        );
        for bad in [
            RecordedMove {
                card_id: 11,
                from: Pile::Cascade(from),
                to: Pile::Cascade(3),
                is_auto: false,
            },
            RecordedMove {
                card_id: 25,
                from: Pile::Cascade(to),
                to: Pile::Cascade(from),
                is_auto: false,
            },
            RecordedMove {
                card_id: 11,
                from: Pile::Cascade(from),
                to: Pile::Cell(0),
                is_auto: false,
            },
            RecordedMove {
                card_id: 10,
                from: Pile::Cascade(from),
                to: Pile::Cell(1),
                is_auto: false,
            },
            RecordedMove {
                card_id: 9,
                from: Pile::Foundation(0),
                to: Pile::Cell(1),
                is_auto: false,
            },
            RecordedMove {
                card_id: 12,
                from: Pile::Cell(0),
                to: Pile::Cell(1),
                is_auto: false,
            },
        ] {
            assert!(p.apply(bad).is_err());
            assert_eq!(
                p.column(usize::from(from)),
                Some(columns[usize::from(from)])
            );
        }
    }
    let p = position(
        [&[25], &[38], &[50, 51], &[], &[], &[], &[], &[]],
        [EMPTY; 4],
        [13, 12, 12, 11],
    );
    assert!(p.automatic().is_none());
    assert!(
        p.apply(RecordedMove {
            card_id: 25,
            from: Pile::Cascade(0),
            to: Pile::Foundation(2),
            is_auto: false
        })
        .is_err()
    );
    assert!(
        p.successors()
            .any(|(a, _)| a.card_id == 25 && a.to == Pile::Foundation(1) && !a.is_auto)
    );
    let mut p = position(
        [&[12], &[25], &[38], &[51], &[], &[], &[], &[]],
        [EMPTY; 4],
        [12; 4],
    );
    for column in 0..4 {
        let edges = p.successors().collect::<Vec<_>>();
        assert_eq!(edges.len(), 1);
        assert!(edges[0].0.is_auto);
        assert_eq!(edges[0].0.from, Pile::Cascade(column));
        assert_eq!(edges[0].0.card_id, [12, 25, 38, 51][usize::from(column)]);
        p = edges[0].1;
        p.validate().unwrap();
    }
    assert!(p.is_goal());
}


/// Independent exact physical state for a tiny exhaustive, no-autoplay reference graph.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
struct Reference {
    columns: [Vec<u8>; 8],
    cells: [u8; 4],
    foundations: [u8; 4],
}


impl Reference {
    /// Apply a single legal move with independent array/Vec mutation and no canonical key.
    fn successors(&self) -> Vec<(RecordedMove, Self)> {
        let mut children = Vec::new();
        for source in 0..12 {
            let id = if source < 8 {
                self.columns[source].last().copied().unwrap_or(EMPTY)
            } else {
                self.cells[source - 8]
            };
            if id == EMPTY {
                continue;
            }
            for destination in 0..16 {
                let legal = if destination < 8 {
                    destination != source
                        && self.columns[destination].len() < 20
                        && self.columns[destination].last().is_none_or(|&top| {
                            top % 13 == id % 13 + 1
                                && matches!(top / 13, 1 | 2) != matches!(id / 13, 1 | 2)
                        })
                } else if destination < 12 {
                    source < 8 && self.cells[destination - 8] == EMPTY
                } else {
                    destination - 12 == usize::from(id / 13)
                        && self.foundations[destination - 12] == id % 13
                };
                if !legal {
                    continue;
                }
                let mut child = self.clone();
                if source < 8 {
                    child.columns[source].pop();
                } else {
                    child.cells[source - 8] = EMPTY;
                }
                if destination < 8 {
                    child.columns[destination].push(id);
                } else if destination < 12 {
                    child.cells[destination - 8] = id;
                } else {
                    child.foundations[destination - 12] += 1;
                }
                let pile = |index| match index {
                    0..=7 => Pile::Cascade(index as u8),
                    8..=11 => Pile::Cell((index - 8) as u8),
                    _ => Pile::Foundation((index - 12) as u8),
                };
                children.push((
                    RecordedMove {
                        card_id: id,
                        from: pile(source),
                        to: pile(destination),
                        is_auto: false,
                    },
                    child,
                ));
            }
        }
        children
    }
}


/// The pruned/automatic graph and independent physical graph both reach the same complete goal.
#[test]
fn freecell_310_small_reference_graph_checks_reachability_and_physical_route() {
    let start = position(
        [&[12], &[25], &[38], &[50, 51], &[], &[], &[], &[]],
        [EMPTY; 4],
        [12, 12, 12, 11],
    );
    let reference = Reference {
        columns: std::array::from_fn(|c| start.column(c).unwrap().to_vec()),
        cells: start.cells,
        foundations: start.foundations,
    };
    let mut seen = std::collections::HashSet::new();
    let mut todo = VecDeque::from([reference.clone()]);
    let mut reached = false;
    while let Some(p) = todo.pop_front() {
        if p.foundations == [13; 4] {
            reached = true;
            break;
        }
        if !seen.insert(p.clone()) {
            continue;
        }
        todo.extend(p.successors().into_iter().map(|(_, state)| state));
    }
    assert!(reached);
    let mut seen = std::collections::HashSet::new();
    let mut todo = VecDeque::from([(start, Vec::new())]);
    let mut route = None;
    while let Some((p, actions)) = todo.pop_front() {
        if p.is_goal() {
            route = Some(actions);
            break;
        }
        if !seen.insert(p) {
            continue;
        }
        for (a, next) in p.successors() {
            let mut actions = actions.clone();
            actions.push(a);
            todo.push_back((next, actions));
        }
    }
    let mut p = start;
    let mut independent = reference;
    for action in route.expect("same reachable full goal") {
        independent = independent
            .successors()
            .into_iter()
            .find_map(|(legal, state)| {
                (legal.card_id == action.card_id
                    && legal.from == action.from
                    && legal.to == action.to)
                    .then_some(state)
            })
            .expect("independently legal physical reference transition");
        assert_eq!(p.exposed(action.from), Some(action.card_id));
        p = p.apply(action).unwrap();
        p.validate().unwrap();
    }
    assert!(p.is_goal());
    assert_eq!(independent.foundations, [13; 4]);
    assert_eq!(independent.cells, [EMPTY; 4]);
    assert!(independent.columns.iter().all(Vec::is_empty));
}
