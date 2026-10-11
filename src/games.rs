//! Optional pure identities and original-deal models, not live game controllers.
//!
//! No feature in this module requires images, QMP, native coordinates or a GUI.
//! A complete model validates its contents, not the truth of acquisition claims.


/// Standard single-deck rank and suit identities; available with `cards`.
pub mod cards;


/// Validated complete original Pyramid arrangements; available with `pyramid`.
#[cfg(feature = "pyramid")]
pub mod pyramid;
