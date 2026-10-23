//! Public non-card consumer connecting independent search and execution features.
#![cfg(all(feature = "search", feature = "execution"))]

use std::{sync::atomic::AtomicBool, time::Duration};
use xsar::{
    execution::{
        ActionPlan, DeliveryReceipt, EffectVerdict, Limits, ObservationPhase, PreconditionVerdict,
        Request, Sequencer,
    },
    search::{self, SearchLimits, SearchMethod, SearchOutcome, SearchProblem},
};


/// Small application setting, with no cards, images, JSON or transport.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
enum Setting {
    Initial,
    DraftSeven,
    AppliedSeven,
}


/// Semantic actions found by the generic search adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Action {
    SetSeven,
    Apply,
}


/// Application input vocabulary; a logical action may own several gestures.
#[derive(Debug)]
enum Gesture {
    Focus,
    WriteSeven,
    PressApply,
}


/// Neutral finite unit-cost adapter with allocation-free successors.
struct Settings;


impl SearchProblem for Settings {
    type State = Setting;
    type Action = Action;
    type Successors = std::option::IntoIter<(Action, Setting)>;


    /// Begin with the untouched draft setting.
    fn initial(&self) -> Setting {
        Setting::Initial
    }


    /// An applied seven is the only successful outcome.
    fn is_goal(&self, state: Setting) -> bool {
        state == Setting::AppliedSeven
    }


    /// Set the draft, then apply it, without Pyramid-specific limits or actions.
    fn successors(&self, state: Setting) -> Self::Successors {
        match state {
            Setting::Initial => Some((Action::SetSeven, Setting::DraftSeven)),
            Setting::DraftSeven => Some((Action::Apply, Setting::AppliedSeven)),
            Setting::AppliedSeven => None,
        }
        .into_iter()
    }
}


/// A generic found route executes through the public neutral sequencer unchanged.
#[test]
fn generic_search_route_drives_two_actions_through_three_verified_gestures() {
    let SearchOutcome::Solved { actions, .. } = search::solve(
        &Settings,
        SearchLimits::default(),
        &AtomicBool::new(false),
        SearchMethod::AStar,
    ) else {
        panic!("finite settings path must solve");
    };
    assert_eq!(&*actions, &[Action::SetSeven, Action::Apply]);
    let plans = actions
        .iter()
        .map(|action| ActionPlan {
            operations: match action {
                Action::SetSeven => vec![Gesture::Focus, Gesture::WriteSeven],
                Action::Apply => vec![Gesture::PressApply],
            },
            settle_delay: Duration::ZERO,
            reobserve_delay: Duration::ZERO,
        })
        .collect();
    let limits = Limits {
        max_actions: 2,
        max_gestures: 3,
        max_captures: 4,
        max_precondition_observations: 1,
        max_effect_observations: 1,
        max_duration: Duration::from_secs(1),
    };
    let mut sequence = Sequencer::new(7u16, plans, limits, Duration::ZERO).unwrap();
    let mut focused = false;
    let mut actual = Setting::Initial;
    loop {
        match sequence.next(&7, Duration::ZERO).unwrap() {
            Request::Observe {
                ticket,
                action_index,
                phase,
            } => match phase {
                ObservationPhase::Precondition => {
                    assert_eq!(
                        actual,
                        if action_index == 0 {
                            Setting::Initial
                        } else {
                            Setting::DraftSeven
                        }
                    );
                    sequence
                        .precondition(ticket, &7, Duration::ZERO, PreconditionVerdict::Ready)
                        .unwrap();
                }
                ObservationPhase::Effect => {
                    assert_eq!(
                        actual,
                        if action_index == 0 {
                            Setting::DraftSeven
                        } else {
                            Setting::AppliedSeven
                        }
                    );
                    sequence
                        .effect(ticket, &7, Duration::ZERO, EffectVerdict::Verified)
                        .unwrap();
                }
            },
            Request::Dispatch {
                ticket, operation, ..
            } => {
                match operation {
                    Gesture::Focus => focused = true,
                    Gesture::WriteSeven => {
                        assert!(focused);
                        actual = Setting::DraftSeven;
                    }
                    Gesture::PressApply => {
                        assert_eq!(actual, Setting::DraftSeven);
                        actual = Setting::AppliedSeven;
                    }
                }
                sequence
                    .delivered(
                        ticket,
                        &7,
                        Duration::ZERO,
                        DeliveryReceipt {
                            attempted: true,
                            acknowledged: true,
                            stop_requested: false,
                        },
                    )
                    .unwrap();
            }
            Request::Wait { ticket, until } => {
                assert_eq!(until, Duration::ZERO);
                sequence.waited(ticket, &7, Duration::ZERO).unwrap();
            }
            Request::ActionVerified { .. } => {}
            Request::Finished => break,
            Request::Pending => panic!("consumer answered every outstanding request"),
        }
    }
    assert_eq!(actual, Setting::AppliedSeven);
    assert_eq!(sequence.statistics().actions_verified, 2);
    assert_eq!(sequence.statistics().gestures_acknowledged, 3);
}
