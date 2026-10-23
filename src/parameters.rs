//! Timing defaults for reusable transport and search, without application policy.
//!
//! Durations retain the established values; they are not measured minimums.
//! Public QMP defaults let consumers share one source while preserving explicit
//! caller overrides. Poll intervals bound responsiveness, not total deadlines.

use std::time::Duration;


/// Ordinary QMP read/write budget; callers may choose a different I/O timeout.
#[cfg(all(feature = "qmp", unix))]
pub const QMP_IO_TIMEOUT: Duration = Duration::from_millis(750);


/// Longer QMP budget for guest PNG generation, independent of ordinary replies.
#[cfg(all(feature = "qmp", unix))]
pub const QMP_SCREENDUMP_TIMEOUT: Duration = Duration::from_secs(5);


/// Pause after absolute pointer movement before pressing a button.
#[cfg(all(feature = "qmp", unix))]
pub const POINTER_SETTLE_DELAY: Duration = Duration::from_millis(100);


/// Ordinary button-down duration; application-specific controls may need longer.
#[cfg(all(feature = "qmp", unix))]
pub const MOUSE_HOLD: Duration = Duration::from_millis(50);


/// Key-down duration before release; delivery still needs separate acknowledgements.
#[cfg(all(feature = "qmp", unix))]
pub const KEY_HOLD: Duration = Duration::from_millis(20);


/// Bound one TCP connection attempt; this does not authorise automatic retries.
#[cfg(feature = "tcp_capture")]
pub(crate) const TCP_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);


/// Independent absolute budget for writing a request or reading each response stage.
#[cfg(feature = "tcp_capture")]
pub(crate) const TCP_SOCKET_TIMEOUT: Duration = Duration::from_secs(10);


/// Maximum blocking socket slice between cancellation and deadline checks.
#[cfg(feature = "tcp_capture")]
pub(crate) const TCP_CANCEL_POLL: Duration = Duration::from_millis(100);


/// Default search time allowance; each caller retains its own explicit limits.
#[cfg(feature = "search")]
pub(crate) const DEFAULT_SEARCH_DURATION: Duration = Duration::from_secs(120);


/// Minimum interval between non-final observer callbacks to bound reporting overhead.
#[cfg(feature = "search")]
pub(crate) const SEARCH_PROGRESS_INTERVAL: Duration = Duration::from_millis(200);
