//! QMP transport with per-I/O timeouts and release-safe input over a Unix socket.
//!
//! Requests carry numeric identifiers; asynchronous events do not satisfy a
//! request. Failed input receives release-only recovery, never a replayed press.
//! Timeouts apply to socket I/O, not to a total command deadline or connection
//! establishment. A stream of events can prolong response collection.

use std::{
    io::{BufRead, BufReader, Write},
    net::Shutdown,
    os::unix::net::UnixStream,
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    sync::Arc,
    thread,
    time::Duration,
};

use serde_json::{Value, json};
use thiserror::Error;

use crate::geometry::{GeometryError, PixelPoint, pixel_to_qmp_axis};


/// Physical stream within one caller-correlated connection and its release-only recovery.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QmpWireConnection {
    /// Original caller connection; its numeric request identifiers start at one.
    Primary,
    /// Fresh release-only recovery stream; its identifiers restart at one.
    ReleaseRecovery,
}


/// Socket-write boundary at which an outgoing request failed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QmpWireStage {
    /// Compact JSON serialization or its socket write failed; partial bytes are possible.
    JsonWrite,
    /// Writing the terminating newline failed after the JSON write completed.
    NewlineWrite,
    /// Flushing the completed newline-delimited request failed.
    Flush,
}


/// Borrowed outgoing wire evidence; write completion never proves acknowledgement or guest effect.
#[derive(Debug)]
pub enum QmpWireEvent<'a> {
    /// Complete intended compact JSON before its first socket-write attempt.
    Attempted {
        /// Original or fresh release-recovery physical stream.
        connection: QmpWireConnection,
        /// Exact numeric identifier serialized in this request.
        id: u64,
        /// Exact compact JSON text; the socket write appends one newline.
        json: &'a str,
    },
    /// The complete JSON, newline and flush succeeded; no response has been inferred.
    Flushed {
        /// Physical stream on which this write completed.
        connection: QmpWireConnection,
        /// Numeric identifier of the completed write.
        id: u64,
    },
    /// A socket-write boundary failed; this preserves the original transport error.
    Failed {
        /// Physical stream on which the attempt failed.
        connection: QmpWireConnection,
        /// Numeric identifier of the failed attempt.
        id: u64,
        /// Last attempted serialization, newline or flush boundary.
        stage: QmpWireStage,
        /// Original borrowed error; observation cannot replace or consume it.
        error: &'a QmpError,
    },
}


/// Passive caller-owned wire observer, installed before capability negotiation.
///
/// Return promptly and perform no guest input. Each original connection should use
/// an observer instance carrying the caller's own connection/session correlation;
/// that same instance follows its fresh release-only recovery. Unwinding observer
/// panics are contained, but process-aborting panics cannot be recovered. The
/// library owns no logging files, environment selection or observer error policy.
pub trait QmpWireObserver: Send + Sync {
    /// Inspect one synchronous borrowed boundary without changing transport outcomes.
    fn observe(&self, event: QmpWireEvent<'_>);
}


/// Named minimum intervals between absolute diagnostic input phases.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticDragTiming {
    /// Wait two seconds between acknowledged source positioning and each held input phase.
    TwoSeconds,
    /// Wait 500 milliseconds between acknowledged source positioning and each held input phase.
    HalfSecond,
}


impl DiagnosticDragTiming {
    /// Return the common source-settle and held-phase minimum interval.
    fn delay(self) -> Duration {
        match self {
            Self::TwoSeconds => Duration::from_secs(2),
            Self::HalfSecond => Duration::from_millis(500),
        }
    }
}


/// Native destination offset fixed by the four-pixel upward-approach diagnostic.
const DIAGNOSTIC_APPROACH_NATIVE_PIXELS: i32 = 4;


/// Caller-selected per-I/O timeouts and input pacing for one QMP connection.
///
/// Hold and settle durations may be zero. Socket timeout durations must be
/// non-zero; connection setup rejects zero before opening the socket.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QmpConfig {
    /// Timeout for each normal socket read/write operation, not an overall deadline.
    pub io_timeout: Duration,
    /// Read timeout while waiting for a PNG screendump acknowledgement.
    pub screendump_timeout: Duration,
    /// Delay after acknowledged pointer movement and before button-down.
    pub pointer_settle_delay: Duration,
    /// Button-down duration used by [`QmpClient::click`].
    pub mouse_hold: Duration,
    /// Key-down duration used by [`QmpClient::press_key_default`].
    pub key_hold: Duration,
}


impl Default for QmpConfig {
    /// Preserve the original controller's per-I/O and input timing defaults.
    fn default() -> Self {
        Self {
            io_timeout: crate::parameters::QMP_IO_TIMEOUT,
            screendump_timeout: crate::parameters::QMP_SCREENDUMP_TIMEOUT,
            pointer_settle_delay: crate::parameters::POINTER_SETTLE_DELAY,
            mouse_hold: crate::parameters::MOUSE_HOLD,
            key_hold: crate::parameters::KEY_HOLD,
        }
    }
}


impl QmpConfig {
    /// Reject invalid socket timeouts before starting a connection exchange.
    fn validate(self) -> Result<Self, QmpError> {
        if self.io_timeout.is_zero() || self.screendump_timeout.is_zero() {
            return Err(QmpError::Protocol(
                "QMP socket timeouts must be non-zero".to_owned(),
            ));
        }

        Ok(self)
    }
}


/// Transport, protocol, input-recovery and capture-path failures.
#[derive(Debug, Error)]
pub enum QmpError {
    /// The Unix stream could not complete an I/O operation.
    #[error("QMP socket I/O failed: {0}")]
    Io(#[from] std::io::Error),
    /// A received packet is not valid JSON.
    #[error("invalid JSON from QMP: {0}")]
    Json(#[from] serde_json::Error),
    /// A packet or connection state violates the expected QMP exchange.
    #[error("invalid QMP protocol response: {0}")]
    Protocol(String),
    /// QEMU returned an explicit command error.
    #[error("QEMU rejected the command: {0}")]
    Command(String),
    /// The requested PNG path is not an absolute, UTF-8, NUL-free path.
    #[error("invalid screendump output path: {0}")]
    CapturePath(String),
    /// The input action failed and a fresh connection could not confirm release.
    #[error("{action}; a fresh QMP connection could not confirm the safety release: {recovery}")]
    InputRecovery {
        /// Original input failure, retained as the causal error.
        action: Box<QmpError>,
        /// Failure encountered while confirming a release on a fresh connection.
        recovery: Box<QmpError>,
    },
    /// The guest input coordinates cannot be mapped into the declared frame.
    #[error(transparent)]
    Geometry(#[from] GeometryError),
}


/// Raw status and pointer information obtained from QEMU before automation.
#[derive(Clone, Debug)]
pub struct QmpProbe {
    /// Response payload from query-status.
    status: Value,
    /// Response payload from query-mice.
    mice: Value,
}


impl QmpProbe {
    /// Returns QEMU's reported run-state label, or "unknown" when absent.
    pub fn run_state(&self) -> &str {
        self.status
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
    }


    /// Reports whether QEMU explicitly says the virtual machine is running.
    pub fn is_running(&self) -> bool {
        self.status.get("running").and_then(Value::as_bool) == Some(true)
    }


    /// Finds the name of the active pointing device when it uses absolute input.
    pub fn current_absolute_pointer_name(&self) -> Option<&str> {
        self.mice.as_array()?.iter().find_map(|mouse| {
            let current = mouse.get("current").and_then(Value::as_bool) == Some(true);
            let absolute = mouse.get("absolute").and_then(Value::as_bool) == Some(true);

            (current && absolute)
                .then(|| mouse.get("name").and_then(Value::as_str))
                .flatten()
        })
    }


    /// Summarises the VM state and active absolute pointer for user-facing logs.
    pub fn summary(&self) -> String {
        match self.current_absolute_pointer_name() {
            Some(pointer) => format!(
                "QEMU state={} | active absolute pointer={pointer}",
                self.run_state()
            ),
            None => format!(
                "QEMU state={} | no active absolute pointer was reported",
                self.run_state()
            ),
        }
    }
}


/// Fresh query-mice identity; this is routing evidence, never guest-effect proof.
#[derive(Clone, Copy, Debug)]
struct DiagnosticPointer {
    /// Non-negative QEMU mouse index accepted by the restricted HMP selector.
    index: i32,
    /// Whether this handler accepts absolute rather than relative motion.
    absolute: bool,
    /// Whether QEMU reports this pointer as current in the fresh inventory.
    current: bool,
}


/// Minimal newline-delimited JSON client for a local QEMU QMP Unix socket.
///
/// It exposes status, pointer, capture and paired-input commands; it does not
/// choose actions or verify their effects in guest software.
pub struct QmpClient {
    /// Buffered response reader sharing the connected Unix socket.
    reader: BufReader<UnixStream>,
    /// Request writer sharing the connected Unix socket.
    writer: UnixStream,
    /// Socket location reused only for release recovery.
    socket_path: PathBuf,
    /// Next checked numeric request identifier.
    next_id: u64,
    /// Whether this connection is still permitted to send normal requests.
    usable: bool,
    /// Timing retained unchanged when release recovery opens a fresh connection.
    config: QmpConfig,
    /// Caller-owned sink; every callback is isolated, preserving this client's unwind markers.
    observer: Option<AssertUnwindSafe<Arc<dyn QmpWireObserver>>>,
    /// Physical stream identity relative to the caller's original connection.
    connection: QmpWireConnection,
}


impl QmpClient {
    /// Opens a QMP socket, validates its greeting, and enables QMP commands.
    pub fn connect(path: &Path) -> Result<Self, QmpError> {
        Self::connect_with_config(path, QmpConfig::default())
    }


    /// Opens and negotiates QMP using caller-selected per-I/O and action timing.
    ///
    /// This does not establish an overall deadline for connect or a command.
    pub fn connect_with_config(path: &Path, config: QmpConfig) -> Result<Self, QmpError> {
        let config = config.validate()?;
        let writer = UnixStream::connect(path)?;
        Self::from_connected_stream_with_config(path, writer, config)
    }


    /// Negotiate QMP with passive outgoing JSON observation, including capability negotiation.
    ///
    /// The shared observer follows release-only recovery without changing timing,
    /// original errors or input policy. See [`QmpWireObserver`] for callback bounds.
    ///
    /// # Errors
    /// Returns the same timeout-configuration, connection, greeting and negotiation
    /// errors as [`Self::connect_with_config`]. Observer return values cannot fail I/O.
    pub fn connect_with_observer(
        path: &Path,
        config: QmpConfig,
        observer: Arc<dyn QmpWireObserver>,
    ) -> Result<Self, QmpError> {
        Self::connect_observed(path, config, Some(observer), QmpWireConnection::Primary)
    }


    /// Open one original or release-recovery stream with its caller-owned sink already attached.
    fn connect_observed(
        path: &Path,
        config: QmpConfig,
        observer: Option<Arc<dyn QmpWireObserver>>,
        connection: QmpWireConnection,
    ) -> Result<Self, QmpError> {
        let config = config.validate()?;
        let writer = UnixStream::connect(path)?;
        Self::from_connected_stream_observed(path, writer, config, observer, connection)
    }


    /// Configures a connected stream with default timing for transport tests.
    #[cfg(test)]
    fn from_connected_stream(path: &Path, writer: UnixStream) -> Result<Self, QmpError> {
        Self::from_connected_stream_with_config(path, writer, QmpConfig::default())
    }


    /// Configures an already-connected stream, validates QMP, and enables commands.
    fn from_connected_stream_with_config(
        path: &Path,
        writer: UnixStream,
        config: QmpConfig,
    ) -> Result<Self, QmpError> {
        Self::from_connected_stream_observed(path, writer, config, None, QmpWireConnection::Primary)
    }


    /// Configure one stream before its greeting and capabilities are exchanged.
    fn from_connected_stream_observed(
        path: &Path,
        writer: UnixStream,
        config: QmpConfig,
        observer: Option<Arc<dyn QmpWireObserver>>,
        connection: QmpWireConnection,
    ) -> Result<Self, QmpError> {
        let config = config.validate()?;
        writer.set_read_timeout(Some(config.io_timeout))?;
        writer.set_write_timeout(Some(config.io_timeout))?;

        let reader_stream = writer.try_clone()?;
        reader_stream.set_read_timeout(Some(config.io_timeout))?;

        let mut client = Self {
            reader: BufReader::new(reader_stream),
            writer,
            socket_path: path.to_path_buf(),
            next_id: 1,
            usable: true,
            config,
            observer: observer.map(AssertUnwindSafe),
            connection,
        };

        let greeting = client.read_packet()?;


        if greeting.get("QMP").is_none() {
            return Err(QmpError::Protocol(
                "the socket did not begin with a QMP greeting".to_owned(),
            ));
        }

        client.execute("qmp_capabilities", None)?;
        Ok(client)
    }


    /// Queries the VM status and available pointing devices.
    pub fn probe(&mut self) -> Result<QmpProbe, QmpError> {
        let status = self.execute("query-status", None)?;
        let mice = self.execute("query-mice", None)?;
        Ok(QmpProbe { status, mice })
    }


    /// Writes the primary display's implicit head 0 to an absolute PNG path.
    ///
    /// This is a read-only guest operation. It sends no pointer or keyboard
    /// events and returns only after QEMU acknowledges the screendump command.
    pub fn screendump_png(&mut self, output_path: &Path) -> Result<(), QmpError> {
        let filename = qmp_capture_filename(output_path)?;
        self.reader
            .get_ref()
            .set_read_timeout(Some(self.config.screendump_timeout))?;
        let capture_result = self
            .execute(
                "screendump",
                Some(json!({
                    "filename": filename,
                    "format": "png"
                })),
            )
            .map(|_| ());
        let restore_result = self
            .reader
            .get_ref()
            .set_read_timeout(Some(self.config.io_timeout));


        match capture_result {
            Err(error) => {
                let _ = restore_result;
                Err(error)
            }
            Ok(()) => {
                restore_result?;
                Ok(())
            }
        }
    }


    /// Moves the absolute pointer without any button or keyboard transition.
    ///
    /// The matching acknowledgement is required before returning. A failed
    /// or uncertain movement is not retried, and this adds no settle delay.
    pub fn move_pointer(
        &mut self,
        guest_point: PixelPoint,
        guest_width: u32,
        guest_height: u32,
    ) -> Result<(), QmpError> {
        let pixel_x = u32::try_from(guest_point.x).map_err(|_| {
            QmpError::Protocol(format!("negative guest X coordinate: {}", guest_point.x))
        })?;
        let pixel_y = u32::try_from(guest_point.y).map_err(|_| {
            QmpError::Protocol(format!("negative guest Y coordinate: {}", guest_point.y))
        })?;
        let x = pixel_to_qmp_axis(pixel_x, guest_width)?;
        let y = pixel_to_qmp_axis(pixel_y, guest_height)?;

        self.execute(
            "input-send-event",
            Some(json!({
                "events": [
                    { "type": "abs", "data": { "axis": "x", "value": x } },
                    { "type": "abs", "data": { "axis": "y", "value": y } }
                ]
            })),
        )?;
        Ok(())
    }


    /// Sends a normal-duration absolute left click through QMP.
    pub fn click(
        &mut self,
        guest_point: PixelPoint,
        guest_width: u32,
        guest_height: u32,
    ) -> Result<(), QmpError> {
        self.click_with_hold(
            guest_point,
            guest_width,
            guest_height,
            self.config.mouse_hold,
        )
    }


    /// Drag between checked guest pixels without releasing at the source.
    ///
    /// The source movement is acknowledged before pressing. Down, held movement
    /// and up are sent before collecting their replies, so delayed ACKs cannot
    /// leave the button held. The predicate must return promptly; false revokes
    /// further movement. Any failure after a down attempt performs release-only
    /// recovery and invalidates this connection, never repeating the transfer.
    /// A safety release may change the guest: interruption does not imply rollback.
    /// Uses the configured mouse hold and pointer settling intervals; blocks on I/O.
    pub fn drag<F>(
        &mut self,
        source: PixelPoint,
        destination: PixelPoint,
        guest_width: u32,
        guest_height: u32,
        mut permitted: F,
    ) -> Result<(), QmpError>
    where
        F: FnMut() -> bool,
    {
        let movement = |point: PixelPoint| -> Result<Value, QmpError> {
            let x = u32::try_from(point.x)
                .map_err(|_| QmpError::Protocol("negative drag X coordinate".into()))?;
            let y = u32::try_from(point.y)
                .map_err(|_| QmpError::Protocol("negative drag Y coordinate".into()))?;
            Ok(json!({"events":[
                {"type":"abs","data":{"axis":"x","value":pixel_to_qmp_axis(x, guest_width)?}},
                {"type":"abs","data":{"axis":"y","value":pixel_to_qmp_axis(y, guest_height)?}}
            ]}))
        };
        let from = movement(source)?;
        let to = movement(destination)?;
        if from == to {
            return Err(QmpError::Protocol("drag endpoints must differ".into()));
        }
        let interrupted = || QmpError::Protocol("drag authority interrupted".into());
        if !permitted() {
            return Err(interrupted());
        }
        self.execute("input-send-event", Some(from))?;
        thread::sleep(self.config.pointer_settle_delay);
        if !permitted() {
            return Err(interrupted());
        }
        let up = json!({"events":[{"type":"btn","data":{"button":"left","down":false}}]});
        let action = (|| {
            let down = self.send_request(
                "input-send-event",
                Some(json!({"events":[
                    {"type":"btn","data":{"button":"left","down":true}}
                ]})),
            )?;
            thread::sleep(self.config.mouse_hold);
            if !permitted() {
                return Err(interrupted());
            }
            let moved = self.send_request("input-send-event", Some(to))?;
            thread::sleep(self.config.pointer_settle_delay);
            if !permitted() {
                return Err(interrupted());
            }
            let released = self.send_request("input-send-event", Some(up.clone()))?;
            self.await_responses(&[down, moved, released]).map(|_| ())
        })();
        action.map_err(|error| self.recover_release(up, error))
    }


    /// Deliver the absolute approach diagnostic with two-second phase intervals.
    ///
    /// Uses [`DiagnosticDragTiming::TwoSeconds`] through
    /// [`Self::diagnostic_drag_with_timing`], retaining this entry point's signature.
    ///
    /// # Errors
    /// Returns the same checked-coordinate, routing, authority and release-recovery
    /// errors as [`Self::diagnostic_drag_with_timing`].
    pub fn diagnostic_drag<F>(
        &mut self,
        source: PixelPoint,
        destination: PixelPoint,
        guest_width: u32,
        guest_height: u32,
        permitted: F,
    ) -> Result<(), QmpError>
    where
        F: FnMut() -> bool,
    {
        self.diagnostic_drag_with_timing(
            source,
            destination,
            (guest_width, guest_height),
            DiagnosticDragTiming::TwoSeconds,
            permitted,
        )
    }


    /// Deliver one absolute diagnostic through a checked four-pixel upward approach.
    ///
    /// Freshly select and verify the unique absolute pointer before positioning
    /// the source. After its ACK, wait the selected interval before a fresh
    /// routing check and LEFT down. Use that same interval between DOWN, approach
    /// ABS, exact destination ABS and LEFT up: four waits across five input packets.
    /// The approach is four native pixels above the destination, at its exact x
    /// and checked y minus four, independently mapped by the normal absolute-axis
    /// conversion. Destinations with y below four refuse without clamping or a
    /// fallback. An approach may map onto an endpoint; mapped endpoint aliases
    /// still refuse.
    /// Each input is a separate request. Collect every held-phase reply only
    /// after UP has been transmitted, with no additional wait after UP. Socket
    /// I/O and synchronous observation can add time to these minimum intervals.
    ///
    /// Routing cannot be pinned across the held interval. Restore the original
    /// pointer only after confirmed UP and while the caller still permits input.
    /// This does not change [`Self::drag`], capture policy or add another click.
    ///
    /// # Errors
    /// Invalid or mapped-alias endpoints, or a destination y below four, fail
    /// before I/O. Ambiguous or changed routing and revoked authority fail before
    /// pressing where possible. Any possible post-DOWN failure preserves its
    /// cause and reselects only the captured target for old/fresh safety UP.
    /// DOWN and motion are never replayed.
    /// STOP can leave the diagnostic pointer selected. Release is never rollback.
    pub fn diagnostic_drag_with_timing<F>(
        &mut self,
        source: PixelPoint,
        destination: PixelPoint,
        frame: (u32, u32),
        timing: DiagnosticDragTiming,
        permitted: F,
    ) -> Result<(), QmpError>
    where
        F: FnMut() -> bool,
    {
        self.diagnostic_drag_with_delay(source, destination, frame, timing.delay(), permitted)
    }


    /// Deliver one absolute diagnostic through exactly four caller-supplied held points.
    ///
    /// Validate and map the source and all four native points before checking
    /// authority or performing I/O. The last point is the exact release endpoint
    /// and must map differently from the source. Intermediate points may repeat
    /// each other or map onto either endpoint; each still receives its own request.
    /// The caller owns the path's geometry and diagnostic meaning.
    ///
    /// Freshly select and verify the unique absolute pointer, position the source
    /// and await its ACK, then wait the selected interval and freshly check routing
    /// before DOWN. Send DOWN, four ABS requests in slice order and UP separately,
    /// with the same selected minimum interval before every subsequent input:
    /// seven input packets and six waits. Collect all six held-phase replies only
    /// after transmitting UP, without an additional post-UP wait. Socket I/O and
    /// synchronous observation may extend these minimum intervals.
    ///
    /// Routing cannot be pinned throughout the held interval. Restore the original
    /// pointer only after confirmed UP while the caller still permits input.
    ///
    /// # Errors
    /// A point count other than four, invalid native coordinates or mapped-alias
    /// source/final endpoints fail before authority checks or I/O. Ambiguous or
    /// changed pointer routing and revoked authority fail before pressing where
    /// possible. Any possible post-DOWN failure preserves its cause and reselects
    /// only the captured pointer for old/fresh safety UP; DOWN and motion are never
    /// replayed. STOP may leave that pointer selected. Release is never rollback.
    pub fn diagnostic_drag_path_with_timing<F>(
        &mut self,
        source: PixelPoint,
        held_points: &[PixelPoint],
        frame: (u32, u32),
        timing: DiagnosticDragTiming,
        permitted: F,
    ) -> Result<(), QmpError>
    where
        F: FnMut() -> bool,
    {
        self.diagnostic_drag_path_with_delay(source, held_points, frame, timing.delay(), permitted)
    }


    /// Validate the complete bounded path before using the common release-safe sequence.
    fn diagnostic_drag_path_with_delay<F: FnMut() -> bool>(
        &mut self,
        source: PixelPoint,
        held_points: &[PixelPoint],
        frame: (u32, u32),
        delay: Duration,
        mut permitted: F,
    ) -> Result<(), QmpError> {
        let [first, second, third, last] = held_points else {
            return Err(QmpError::Protocol(
                "diagnostic absolute path requires exactly four held points".into(),
            ));
        };
        let from = Self::diagnostic_absolute_motion(source, frame)?;
        let movements = [first, second, third, last]
            .into_iter()
            .map(|&point| Self::diagnostic_absolute_motion(point, frame))
            .collect::<Result<Vec<_>, _>>()?;
        if movements.last() == Some(&from) {
            return Err(QmpError::Protocol("drag endpoints must differ".into()));
        }
        self.diagnostic_held_input(from, movements, true, delay, &mut permitted)
    }


    /// Deliver the same checked ABS sequence with independently selected diagnostic pacing.
    fn diagnostic_drag_with_delay<F: FnMut() -> bool>(
        &mut self,
        source: PixelPoint,
        destination: PixelPoint,
        frame: (u32, u32),
        delay: Duration,
        mut permitted: F,
    ) -> Result<(), QmpError> {
        let from = Self::diagnostic_absolute_motion(source, frame)?;
        let to = Self::diagnostic_absolute_motion(destination, frame)?;
        if from == to {
            return Err(QmpError::Protocol("drag endpoints must differ".into()));
        }
        let approach_y = destination
            .y
            .checked_sub(DIAGNOSTIC_APPROACH_NATIVE_PIXELS)
            .filter(|&y| y >= 0)
            .ok_or_else(|| {
                QmpError::Protocol("diagnostic approach requires destination y >= 4".into())
            })?;
        let approach =
            Self::diagnostic_absolute_motion(PixelPoint::new(destination.x, approach_y), frame)?;
        let interrupted = || QmpError::Protocol("diagnostic input authority interrupted".into());
        if !permitted() {
            return Err(interrupted());
        }
        let pointers = self.diagnostic_pointers()?;
        let original = pointers
            .iter()
            .find(|pointer| pointer.current)
            .copied()
            .ok_or_else(|| {
                QmpError::Protocol("query-mice did not identify a current pointer".into())
            })?;
        let selected = Self::diagnostic_unique_pointer(&pointers, true)?;
        self.diagnostic_select_pointer(selected, &mut permitted)?;
        if !permitted() {
            return Err(interrupted());
        }
        self.execute("input-send-event", Some(from))?;
        Self::diagnostic_wait(delay, &mut permitted)?;
        let fresh = self.diagnostic_pointers()?;
        if !permitted() {
            return Err(interrupted());
        }
        if !fresh.iter().any(|pointer| {
            pointer.current
                && pointer.index == selected.index
                && pointer.absolute == selected.absolute
        }) || Self::diagnostic_unique_pointer(&fresh, true)?.index != selected.index
        {
            return Err(QmpError::Protocol(
                "diagnostic pointer changed before button down".into(),
            ));
        }
        let up = json!({"events":[{"type":"btn","data":{"button":"left","down":false}}]});
        let action = (|| {
            let down = self.send_request(
                "input-send-event",
                Some(json!({"events":[{"type":"btn","data":{"button":"left","down":true}}]})),
            )?;
            Self::diagnostic_wait(delay, &mut permitted)?;
            let approached = self.send_request("input-send-event", Some(approach))?;
            Self::diagnostic_wait(delay, &mut permitted)?;
            let moved = self.send_request("input-send-event", Some(to))?;
            Self::diagnostic_wait(delay, &mut permitted)?;
            let released = self.send_request("input-send-event", Some(up.clone()))?;
            self.await_responses(&[down, approached, moved, released])
                .map(|_| ())
        })();
        action.map_err(|error| self.recover_diagnostic_release(selected, up, error))?;
        if !permitted() {
            return Err(interrupted());
        }
        if original.index != selected.index {
            self.diagnostic_select_pointer(original, &mut permitted)?;
        }
        Ok(())
    }


    /// Probe relative movement with ten small, direction-only held increments.
    ///
    /// Validate both native endpoints, explicitly select the unique absolute
    /// pointer to position the source, then freshly select and verify the unique
    /// relative pointer before DOWN. Each of ten REL requests uses two device
    /// counts per nonzero direction axis; their total is not the native endpoint
    /// delta and does not promise arrival at the destination. Guest acceleration
    /// and device scaling remain outside this transport's guarantees.
    ///
    /// Separate all source/DOWN/REL/UP phases by at least two seconds. Collect
    /// every held-phase reply only after UP. After confirmed UP, restore the
    /// original pointer only while permitted. No additional click is included.
    /// QMP cannot pin BTN/REL to one handler throughout the held interval.
    ///
    /// # Errors
    /// Invalid/zero endpoints or ambiguous routing refuse before pressing.
    /// Possible post-DOWN failure preserves its cause and reselects only the
    /// captured target for old/fresh safety UP. Never replay DOWN or REL, or
    /// restore the original pointer after failure or STOP. Release is not rollback.
    pub fn diagnostic_relative_drag<F: FnMut() -> bool>(
        &mut self,
        source: PixelPoint,
        destination: PixelPoint,
        width: u32,
        height: u32,
        permitted: F,
    ) -> Result<(), QmpError> {
        self.diagnostic_relative_drag_with_delay(
            source,
            destination,
            (width, height),
            Duration::from_secs(2),
            permitted,
        )
    }


    /// Prepare a bounded direction probe before I/O, preserving native endpoint validation.
    fn diagnostic_relative_drag_with_delay<F: FnMut() -> bool>(
        &mut self,
        source: PixelPoint,
        destination: PixelPoint,
        frame: (u32, u32),
        delay: Duration,
        mut permitted: F,
    ) -> Result<(), QmpError> {
        let from = Self::diagnostic_absolute_motion(source, frame)?;
        let _ = Self::diagnostic_absolute_motion(destination, frame)?;
        let dx = destination
            .x
            .checked_sub(source.x)
            .ok_or_else(|| QmpError::Protocol("relative X delta overflow".into()))?;
        let dy = destination
            .y
            .checked_sub(source.y)
            .ok_or_else(|| QmpError::Protocol("relative Y delta overflow".into()))?;
        if dx == 0 && dy == 0 {
            return Err(QmpError::Protocol(
                "relative drag endpoints must differ".into(),
            ));
        }
        let movement = json!({"events":[
            {"type":"rel","data":{"axis":"x","value":dx.signum() * 2}},
            {"type":"rel","data":{"axis":"y","value":dy.signum() * 2}}
        ]});
        self.diagnostic_held_input(from, vec![movement; 10], false, delay, &mut permitted)
    }


    /// Map one validated native point without rounding, clamping or endpoint substitution.
    fn diagnostic_absolute_motion(point: PixelPoint, frame: (u32, u32)) -> Result<Value, QmpError> {
        let x = u32::try_from(point.x)
            .map_err(|_| QmpError::Protocol("negative drag X coordinate".into()))?;
        let y = u32::try_from(point.y)
            .map_err(|_| QmpError::Protocol("negative drag Y coordinate".into()))?;
        Ok(json!({"events":[
            {"type":"abs","data":{"axis":"x","value":pixel_to_qmp_axis(x, frame.0)?}},
            {"type":"abs","data":{"axis":"y","value":pixel_to_qmp_axis(y, frame.1)?}}
        ]}))
    }


    /// Select checked routing, send one bounded held sequence, then conditionally restore.
    fn diagnostic_held_input<F: FnMut() -> bool>(
        &mut self,
        from: Value,
        movements: Vec<Value>,
        absolute: bool,
        delay: Duration,
        permitted: &mut F,
    ) -> Result<(), QmpError> {
        Self::diagnostic_wait(Duration::ZERO, permitted)?;
        let pointers = self.diagnostic_pointers()?;
        let original = pointers
            .iter()
            .find(|pointer| pointer.current)
            .copied()
            .ok_or_else(|| {
                QmpError::Protocol("query-mice did not identify a current pointer".into())
            })?;
        let source_pointer = Self::diagnostic_unique_pointer(&pointers, true)?;
        if !absolute {
            let _ = Self::diagnostic_unique_pointer(&pointers, false)?;
        }
        self.diagnostic_select_pointer(source_pointer, permitted)?;
        Self::diagnostic_wait(Duration::ZERO, permitted)?;
        self.execute("input-send-event", Some(from))?;
        let selected = if absolute {
            source_pointer
        } else {
            Self::diagnostic_wait(Duration::ZERO, permitted)?;
            let pointers = self.diagnostic_pointers()?;
            let selected = Self::diagnostic_unique_pointer(&pointers, false)?;
            self.diagnostic_select_pointer(selected, permitted)?;
            selected
        };
        Self::diagnostic_wait(delay, permitted)?;
        let fresh = self.diagnostic_pointers()?;
        Self::diagnostic_wait(Duration::ZERO, permitted)?;
        if !fresh.iter().any(|pointer| {
            pointer.current
                && pointer.index == selected.index
                && pointer.absolute == selected.absolute
        }) || Self::diagnostic_unique_pointer(&fresh, absolute)?.index != selected.index
        {
            return Err(QmpError::Protocol(
                "diagnostic pointer changed before button down".into(),
            ));
        }
        let up = json!({"events":[{"type":"btn","data":{"button":"left","down":false}}]});
        let action = (|| {
            let mut ids = Vec::with_capacity(movements.len() + 2);
            ids.push(self.send_request(
                "input-send-event",
                Some(json!({"events":[{"type":"btn","data":{"button":"left","down":true}}]})),
            )?);
            for movement in movements {
                Self::diagnostic_wait(delay, permitted)?;
                ids.push(self.send_request("input-send-event", Some(movement))?);
            }
            Self::diagnostic_wait(delay, permitted)?;
            ids.push(self.send_request("input-send-event", Some(up.clone()))?);
            self.await_responses(&ids).map(|_| ())
        })();
        action.map_err(|error| self.recover_diagnostic_release(selected, up, error))?;
        Self::diagnostic_wait(Duration::ZERO, permitted)?;
        if original.index != selected.index {
            self.diagnostic_select_pointer(original, permitted)?;
        }
        Ok(())
    }


    /// Refuse missing or ambiguous fresh handlers instead of guessing a mouse index.
    fn diagnostic_unique_pointer(
        pointers: &[DiagnosticPointer],
        absolute: bool,
    ) -> Result<DiagnosticPointer, QmpError> {
        let label = if absolute { "absolute" } else { "relative" };
        let mut choices = pointers
            .iter()
            .filter(|pointer| pointer.absolute == absolute);
        let selected = choices
            .next()
            .copied()
            .ok_or_else(|| QmpError::Protocol(format!("no {label} pointer is available")))?;
        if choices.next().is_some() {
            return Err(QmpError::Protocol(format!(
                "multiple {label} pointers require an explicit routing decision"
            )));
        }
        Ok(selected)
    }


    /// Read a strict pointer inventory with unique non-negative indices and one current pointer.
    fn diagnostic_pointers(&mut self) -> Result<Vec<DiagnosticPointer>, QmpError> {
        let value = self.execute("query-mice", None)?;
        let entries = value
            .as_array()
            .ok_or_else(|| QmpError::Protocol("query-mice did not return an array".into()))?;
        let mut pointers: Vec<DiagnosticPointer> = Vec::new();
        for item in entries {
            let index = item
                .get("index")
                .and_then(Value::as_i64)
                .and_then(|n| i32::try_from(n).ok())
                .filter(|n| *n >= 0)
                .ok_or_else(|| QmpError::Protocol("query-mice has an invalid index".into()))?;
            let absolute = item
                .get("absolute")
                .and_then(Value::as_bool)
                .ok_or_else(|| QmpError::Protocol("query-mice has no absolute flag".into()))?;
            let current = item
                .get("current")
                .and_then(Value::as_bool)
                .ok_or_else(|| QmpError::Protocol("query-mice has no current flag".into()))?;
            if pointers.iter().any(|p| p.index == index) {
                return Err(QmpError::Protocol(
                    "query-mice has duplicate indices".into(),
                ));
            }
            pointers.push(DiagnosticPointer {
                index,
                absolute,
                current,
            });
        }
        if pointers.iter().filter(|p| p.current).count() != 1 {
            return Err(QmpError::Protocol(
                "query-mice must identify exactly one current pointer".into(),
            ));
        }
        Ok(pointers)
    }


    /// Select one freshly validated mouse index, guarding each boundary and verifying the outcome.
    fn diagnostic_select_pointer<F: FnMut() -> bool>(
        &mut self,
        choice: DiagnosticPointer,
        permitted: &mut F,
    ) -> Result<(), QmpError> {
        Self::diagnostic_wait(Duration::ZERO, permitted)?;
        let pointers = self.diagnostic_pointers()?;
        Self::diagnostic_wait(Duration::ZERO, permitted)?;
        if !pointers
            .iter()
            .any(|p| p.index == choice.index && p.absolute == choice.absolute)
        {
            return Err(QmpError::Protocol(
                "the selected pointer identity is no longer available".into(),
            ));
        }
        let answer = self.execute(
            "human-monitor-command",
            Some(json!({"command-line":format!("mouse_set {}",choice.index)})),
        )?;
        if answer.as_str() != Some("") {
            return Err(QmpError::Command(format!(
                "mouse_set returned unexpected output: {answer}"
            )));
        }
        Self::diagnostic_wait(Duration::ZERO, permitted)?;
        let pointers = self.diagnostic_pointers()?;
        Self::diagnostic_wait(Duration::ZERO, permitted)?;
        if !pointers
            .iter()
            .any(|p| p.index == choice.index && p.absolute == choice.absolute && p.current)
        {
            return Err(QmpError::Protocol(
                "mouse_set did not activate the requested pointer".into(),
            ));
        }
        Ok(())
    }


    /// Reselect only for release after uncertain diagnostic input; never replay a press or motion.
    ///
    /// Old-stream selection/release are best effort without waiting for outstanding
    /// gesture ACKs. The fresh stream validates/reselects the captured target index
    /// before an acknowledged release. STOP does not restore the original pointer.
    fn recover_diagnostic_release(
        &mut self,
        selected: DiagnosticPointer,
        up: Value,
        action: QmpError,
    ) -> QmpError {
        let _ = self.send_request(
            "human-monitor-command",
            Some(json!({"command-line":format!("mouse_set {}",selected.index)})),
        );
        let _ = self.send_request("input-send-event", Some(up.clone()));
        self.usable = false;
        let _ = self.writer.shutdown(Shutdown::Both);
        let _ = self.reader.get_ref().shutdown(Shutdown::Both);
        let recovery = Self::connect_observed(
            &self.socket_path,
            self.config,
            self.observer.as_ref().map(|observer| observer.0.clone()),
            QmpWireConnection::ReleaseRecovery,
        )
        .and_then(|mut fresh| {
            fresh.diagnostic_select_pointer(selected, &mut || true)?;
            fresh.execute("input-send-event", Some(up)).map(|_| ())
        });
        match recovery {
            Ok(()) => action,
            Err(recovery) => QmpError::InputRecovery {
                action: Box::new(action),
                recovery: Box::new(recovery),
            },
        }
    }


    /// Send one guarded left down/up pair at the current pointer without moving it.
    ///
    /// The caller owns fresh position, scene and effect authority. The prompt
    /// predicate is checked before down and around the configured mouse hold;
    /// this method never supplies a destination, waits for later game policy,
    /// captures a frame or repeats a down. Replies are collected only after up.
    ///
    /// # Errors
    /// An unusable connection or initial revocation fails before input. Any possible
    /// post-down failure uses the existing old/fresh release-only recovery and
    /// invalidates this connection. Recovery release is not rollback.
    pub fn click_current_guarded<F: FnMut() -> bool>(
        &mut self,
        mut permitted: F,
    ) -> Result<(), QmpError> {
        if !self.usable {
            return Err(QmpError::Protocol(
                "the QMP connection is no longer usable".into(),
            ));
        }
        if !permitted() {
            return Err(QmpError::Protocol(
                "diagnostic input authority interrupted".into(),
            ));
        }
        let up = json!({"events":[{"type":"btn","data":{"button":"left","down":false}}]});
        let click = (|| {
            let down = self.send_request(
                "input-send-event",
                Some(json!({"events":[{"type":"btn","data":{"button":"left","down":true}}]})),
            )?;
            Self::diagnostic_wait(self.config.mouse_hold, &mut permitted)?;
            let released = self.send_request("input-send-event", Some(up.clone()))?;
            self.await_responses(&[down, released]).map(|_| ())
        })();
        click.map_err(|error| self.recover_release(up, error))
    }


    /// Check diagnostic authority around one explicitly requested blocking delay.
    fn diagnostic_wait<F: FnMut() -> bool>(
        duration: Duration,
        permitted: &mut F,
    ) -> Result<(), QmpError> {
        if !permitted() {
            return Err(QmpError::Protocol(
                "diagnostic input authority interrupted".into(),
            ));
        }
        thread::sleep(duration);
        if !permitted() {
            return Err(QmpError::Protocol(
                "diagnostic input authority interrupted".into(),
            ));
        }
        Ok(())
    }


    /// Converts a guest pixel position and sends one absolute left click
    /// using the caller-selected button-down duration.
    pub fn click_with_hold(
        &mut self,
        guest_point: PixelPoint,
        guest_width: u32,
        guest_height: u32,
        hold: Duration,
    ) -> Result<(), QmpError> {
        let pixel_x = u32::try_from(guest_point.x).map_err(|_| {
            QmpError::Protocol(format!("negative guest X coordinate: {}", guest_point.x))
        })?;
        let pixel_y = u32::try_from(guest_point.y).map_err(|_| {
            QmpError::Protocol(format!("negative guest Y coordinate: {}", guest_point.y))
        })?;
        let x = pixel_to_qmp_axis(pixel_x, guest_width)?;
        let y = pixel_to_qmp_axis(pixel_y, guest_height)?;

        // Movement is its own synchronised QMP command. Waiting for its ACK
        // prevents the button transition from racing the absolute position.
        self.execute(
            "input-send-event",
            Some(json!({
                "events": [
                    { "type": "abs", "data": { "axis": "x", "value": x } },
                    { "type": "abs", "data": { "axis": "y", "value": y } }
                ]
            })),
        )?;
        thread::sleep(self.config.pointer_settle_delay);

        self.send_press_release(
            json!({
                "events": [
                    { "type": "btn", "data": { "button": "left", "down": true } }
                ]
            }),
            json!({
                "events": [
                    { "type": "btn", "data": { "button": "left", "down": false } }
                ]
            }),
            hold,
        )
    }


    /// Sends a caller-selected QEMU qcode with the configured default hold time.
    pub fn press_key_default(&mut self, qcode: &str) -> Result<(), QmpError> {
        self.press_key(qcode, self.config.key_hold)
    }


    /// Sends a caller-selected QEMU qcode and its release without replaying down.
    ///
    /// QEMU validates the qcode. An empty string is rejected before sending input.
    pub fn press_key(&mut self, qcode: &str, hold: Duration) -> Result<(), QmpError> {
        if qcode.is_empty() {
            return Err(QmpError::Protocol(
                "QEMU qcode must be non-empty".to_owned(),
            ));
        }

        self.send_press_release(
            json!({
                "events": [
                    {
                        "type": "key",
                        "data": {
                            "down": true,
                            "key": { "type": "qcode", "data": qcode }
                        }
                    }
                ]
            }),
            json!({
                "events": [
                    {
                        "type": "key",
                        "data": {
                            "down": false,
                            "key": { "type": "qcode", "data": qcode }
                        }
                    }
                ]
            }),
            hold,
        )
    }


    /// Sends one identified QMP command and returns its matching response payload.
    fn execute(&mut self, command: &str, arguments: Option<Value>) -> Result<Value, QmpError> {
        let id = self.send_request(command, arguments)?;
        let mut responses = self.await_responses(&[id])?;
        Ok(responses.remove(0))
    }


    /// Writes one identified request without waiting for its response.
    fn send_request(&mut self, command: &str, arguments: Option<Value>) -> Result<u64, QmpError> {
        if !self.usable {
            return Err(QmpError::Protocol(
                "the QMP connection is no longer usable".to_owned(),
            ));
        }

        let id = self.next_id;
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or_else(|| QmpError::Protocol("QMP request id exhausted".to_owned()))?;

        let mut packet = json!({
            "execute": command,
            "id": id,
        });


        if let Some(arguments) = arguments {
            packet["arguments"] = arguments;
        }

        let wire = if self.observer.is_some() {
            Some(serde_json::to_string(&packet)?)
        } else {
            None
        };
        if let Some(json) = wire.as_deref() {
            self.observe_wire(QmpWireEvent::Attempted {
                connection: self.connection,
                id,
                json,
            });
        }
        let mut stage = QmpWireStage::JsonWrite;
        let result = (|| -> Result<u64, QmpError> {
            // Serialize the same immutable Value through the original socket writer,
            // retaining its JSON-versus-I/O error classification and framing.
            serde_json::to_writer(&mut self.writer, &packet)?;
            stage = QmpWireStage::NewlineWrite;
            self.writer.write_all(b"\n")?;
            stage = QmpWireStage::Flush;
            self.writer.flush()?;
            Ok(id)
        })();
        if wire.is_some() {
            self.observe_wire(match &result {
                Ok(_) => QmpWireEvent::Flushed {
                    connection: self.connection,
                    id,
                },
                Err(error) => QmpWireEvent::Failed {
                    connection: self.connection,
                    id,
                    stage,
                    error,
                },
            });
        }
        result
    }


    /// Contain an unwinding observer panic without replacing transport errors or recovery.
    fn observe_wire(&self, event: QmpWireEvent<'_>) {
        if let Some(observer) = &self.observer {
            let _ = catch_unwind(AssertUnwindSafe(|| observer.0.observe(event)));
        }
    }


    /// Collects successful responses in request order. A command error is
    /// returned immediately so release recovery retains the causal failure.
    fn await_responses(&mut self, ids: &[u64]) -> Result<Vec<Value>, QmpError> {
        let mut responses = vec![None; ids.len()];


        while responses.iter().any(Option::is_none) {
            let response = self.read_packet()?;


            if response.get("event").is_some() && response.get("id").is_none() {
                // QMP events are asynchronous and are unrelated to these requests.
                continue;
            }

            let response_id = response.get("id").and_then(Value::as_u64).ok_or_else(|| {
                QmpError::Protocol("response has no numeric request id".to_owned())
            })?;
            let response_index = ids
                .iter()
                .position(|id| *id == response_id)
                .ok_or_else(|| {
                    QmpError::Protocol(format!("response has unexpected request id {response_id}"))
                })?;


            if let Some(error) = response.get("error") {
                let description = error
                    .get("desc")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown QMP command error");
                return Err(QmpError::Command(description.to_owned()));
            }
            let payload = response
                .get("return")
                .cloned()
                .ok_or_else(|| QmpError::Protocol("response has no return value".to_owned()))?;


            if responses[response_index].replace(payload).is_some() {
                return Err(QmpError::Protocol(format!(
                    "duplicate response for request id {response_id}"
                )));
            }
        }

        Ok(responses
            .into_iter()
            .map(|response| response.expect("all requested QMP responses were collected"))
            .collect())
    }


    /// Never wait for the down response before transmitting the matching up.
    fn send_press_release(
        &mut self,
        down_arguments: Value,
        up_arguments: Value,
        hold: Duration,
    ) -> Result<(), QmpError> {
        let down_id = match self.send_request("input-send-event", Some(down_arguments)) {
            Ok(id) => id,
            Err(error) => return Err(self.recover_release(up_arguments, error)),
        };

        thread::sleep(hold);


        let up_id = match self.send_request("input-send-event", Some(up_arguments.clone())) {
            Ok(id) => id,
            Err(error) => return Err(self.recover_release(up_arguments, error)),
        };


        match self.await_responses(&[down_id, up_id]) {
            Ok(_) => Ok(()),
            Err(error) => Err(self.recover_release(up_arguments, error)),
        }
    }


    /// A failed write or ACK leaves the guest's input state uncertain. Send
    /// only an idempotent release on the old stream, then close it and
    /// confirm the release once on a fresh QMP connection. Never retry down.
    fn recover_release(&mut self, up_arguments: Value, action: QmpError) -> QmpError {
        let _ = self.send_request("input-send-event", Some(up_arguments.clone()));
        self.usable = false;
        let _ = self.writer.shutdown(Shutdown::Both);
        let _ = self.reader.get_ref().shutdown(Shutdown::Both);

        let recovery = Self::connect_observed(
            &self.socket_path,
            self.config,
            self.observer.as_ref().map(|observer| observer.0.clone()),
            QmpWireConnection::ReleaseRecovery,
        )
        .and_then(|mut fresh| {
            fresh
                .execute("input-send-event", Some(up_arguments))
                .map(|_| ())
        });


        match recovery {
            Ok(()) => action,
            Err(recovery) => QmpError::InputRecovery {
                action: Box::new(action),
                recovery: Box::new(recovery),
            },
        }
    }


    /// Reads and decodes one newline-delimited JSON packet from QMP.
    fn read_packet(&mut self) -> Result<Value, QmpError> {
        let mut line = String::new();
        let bytes_read = self.reader.read_line(&mut line)?;


        if bytes_read == 0 {
            return Err(QmpError::Protocol("QMP socket closed".to_owned()));
        }

        Ok(serde_json::from_str(&line)?)
    }
}


/// Validate a capture path before serialising it into a screendump request.
fn qmp_capture_filename(path: &Path) -> Result<&str, QmpError> {
    if !path.is_absolute() {
        return Err(QmpError::CapturePath(
            "the path must be absolute".to_owned(),
        ));
    }

    let filename = path
        .to_str()
        .ok_or_else(|| QmpError::CapturePath("the path must contain valid UTF-8".to_owned()))?;


    if filename.contains('\0') {
        return Err(QmpError::CapturePath(
            "the path must not contain a NUL byte".to_owned(),
        ));
    }

    Ok(filename)
}


#[cfg(test)]
mod tests {
    //! Mock transport checks for capture arguments and release-safe input ordering.
    use std::{
        ffi::OsString,
        fs,
        io::{ErrorKind, Read},
        os::unix::ffi::OsStringExt,
        os::unix::net::UnixListener,
        process,
        sync::atomic::{AtomicU64, Ordering},
        time::{Duration, Instant},
    };

    use super::*;


    mod drag_tests;


    mod diagnostic_tests;


    mod double_absolute_tests;


    mod path_absolute_tests;


    mod relative_tests;


    /// Process-local suffix used to keep concurrent mock socket paths distinct.
    static NEXT_SOCKET_ID: AtomicU64 = AtomicU64::new(1);


    /// Filesystem pathname guard for a mock Unix socket.
    struct SocketFile(
        /// Socket path removed when the guard is dropped.
        PathBuf,
    );


    impl Drop for SocketFile {
        /// Remove the mock socket pathname when the test guard leaves scope.
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }


    /// Write and flush one newline-delimited JSON packet from the mock server.
    fn write_packet(stream: &mut UnixStream, packet: &Value) {
        serde_json::to_writer(&mut *stream, packet).expect("serialize mock QMP packet");
        stream.write_all(b"\n").expect("write mock QMP newline");
        stream.flush().expect("flush mock QMP packet");
    }


    /// Read and decode a complete request from the mock QMP client.
    fn read_request(reader: &mut BufReader<UnixStream>) -> Value {
        let mut line = String::new();
        let bytes = reader.read_line(&mut line).expect("read mock QMP request");
        assert_ne!(bytes, 0, "mock QMP client closed unexpectedly");
        serde_json::from_str(&line).expect("decode mock QMP request")
    }


    /// Extract the numeric request identifier required by mock responses.
    fn request_id(request: &Value) -> u64 {
        request
            .get("id")
            .and_then(Value::as_u64)
            .expect("request has numeric id")
    }


    /// Send an empty successful response for the selected request.
    fn acknowledge(stream: &mut UnixStream, request: &Value) {
        write_packet(stream, &json!({ "return": {}, "id": request_id(request) }));
    }


    /// Exchange the greeting and capability negotiation with a test client.
    fn start_mock_qmp(stream: &mut UnixStream, reader: &mut BufReader<UnixStream>) {
        write_packet(
            stream,
            &json!({
                "QMP": {
                    "version": { "qemu": { "major": 11, "minor": 1, "micro": 1 } },
                    "capabilities": []
                }
            }),
        );
        let capabilities = read_request(reader);
        assert_eq!(capabilities["execute"], "qmp_capabilities");
        acknowledge(stream, &capabilities);
    }


    /// Accept a mock connection within a fixed deadline.
    fn accept_with_timeout(listener: &UnixListener) -> UnixStream {
        let deadline = Instant::now() + Duration::from_secs(2);


        loop {
            match listener.accept() {
                Ok((stream, _)) => return stream,
                Err(error) if error.kind() == ErrorKind::WouldBlock => {
                    assert!(
                        Instant::now() < deadline,
                        "timed out waiting for QMP client"
                    );
                    thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("accept mock QMP client: {error}"),
            }
        }
    }


    /// Require one input event per press or release request and return it.
    fn first_input_event(request: &Value) -> &Value {
        assert_eq!(request["execute"], "input-send-event");
        let events = request["arguments"]["events"]
            .as_array()
            .expect("input-send-event has an events array");
        assert_eq!(events.len(), 1, "down and up must use separate commands");
        &events[0]
    }


    /// Verifies that only an explicit running status passes the probe check.
    #[test]
    fn probe_requires_qemu_to_report_running() {
        let running = QmpProbe {
            status: json!({ "running": true, "status": "running" }),
            mice: json!([]),
        };
        let paused = QmpProbe {
            status: json!({ "running": false, "status": "paused" }),
            mice: json!([]),
        };

        assert!(running.is_running());
        assert!(!paused.is_running());
    }


    /// Verifies that the probe selects the current absolute pointing device.
    #[test]
    fn probe_accepts_only_the_current_absolute_pointer() {
        let probe = QmpProbe {
            status: json!({ "running": true, "status": "running" }),
            mice: json!([
                { "name": "relative", "current": false, "absolute": false },
                { "name": "tablet", "current": true, "absolute": true }
            ]),
        };

        assert_eq!(probe.current_absolute_pointer_name(), Some("tablet"));
    }


    /// Validate timeout configuration before even attempting to open a socket.
    #[test]
    fn rejects_zero_socket_timeouts_before_connecting() {
        let mut config = QmpConfig {
            io_timeout: Duration::ZERO,
            ..QmpConfig::default()
        };
        assert!(matches!(
            QmpClient::connect_with_config(Path::new("does-not-exist"), config),
            Err(QmpError::Protocol(_))
        ));
        config.io_timeout = Duration::from_millis(500);
        config.screendump_timeout = Duration::ZERO;
        assert!(matches!(
            QmpClient::connect_with_config(Path::new("does-not-exist"), config),
            Err(QmpError::Protocol(_))
        ));
    }


    /// Honor custom timing and arbitrary qcodes without transmitting an empty key.
    #[test]
    fn uses_custom_config_and_caller_selected_qcode() {
        let (client_stream, mut server_stream) = UnixStream::pair().expect("create socket pair");
        server_stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("set server timeout");
        let server_reader = server_stream.try_clone().expect("clone server stream");
        let server = thread::spawn(move || {
            let mut reader = BufReader::new(server_reader);
            start_mock_qmp(&mut server_stream, &mut reader);
            let down = read_request(&mut reader);
            assert_eq!(
                first_input_event(&down)["data"]["key"],
                json!({ "type": "qcode", "data": "space" })
            );
            assert_eq!(first_input_event(&down)["data"]["down"], true);
            let up = read_request(&mut reader);
            assert_eq!(
                first_input_event(&up)["data"]["key"],
                json!({ "type": "qcode", "data": "space" })
            );
            assert_eq!(first_input_event(&up)["data"]["down"], false);
            acknowledge(&mut server_stream, &up);
            acknowledge(&mut server_stream, &down);
        });
        let config = QmpConfig {
            io_timeout: Duration::from_millis(500),
            screendump_timeout: Duration::from_secs(3),
            pointer_settle_delay: Duration::ZERO,
            mouse_hold: Duration::ZERO,
            key_hold: Duration::ZERO,
        };
        let mut client = QmpClient::from_connected_stream_with_config(
            Path::new("mock-qmp"),
            client_stream,
            config,
        )
        .expect("connect configured client");
        assert_eq!(
            client.reader.get_ref().read_timeout().unwrap(),
            Some(config.io_timeout)
        );
        assert_eq!(
            client.writer.write_timeout().unwrap(),
            Some(config.io_timeout)
        );
        assert!(matches!(
            client.press_key_default(""),
            Err(QmpError::Protocol(_))
        ));
        client
            .press_key_default("space")
            .expect("send caller-selected qcode");
        server.join().expect("join custom config mock");
    }


    /// Movement validates bounds and sends exactly two absolute events once.
    #[test]
    fn pointer_movement_rejects_invalid_points_and_sends_no_click() {
        let (client_stream, mut server_stream) = UnixStream::pair().expect("create socket pair");
        server_stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("set mock server timeout");
        let server_reader = server_stream.try_clone().expect("clone mock server");
        let server = thread::spawn(move || {
            let mut reader = BufReader::new(server_reader);
            start_mock_qmp(&mut server_stream, &mut reader);
            let request = read_request(&mut reader);
            assert_eq!(request["execute"], "input-send-event");
            assert_eq!(
                request["arguments"],
                json!({ "events": [
                { "type": "abs", "data": { "axis": "x", "value": 341 } },
                { "type": "abs", "data": { "axis": "y", "value": 15169 } }
            ] })
            );
            acknowledge(&mut server_stream, &request);
            let mut line = String::new();
            assert_eq!(reader.read_line(&mut line).expect("read after movement"), 0);
        });
        let mut client = QmpClient::from_connected_stream(Path::new("mock-qmp"), client_stream)
            .expect("connect mock client");


        for (point, width, height) in [
            (PixelPoint::new(-1, 500), 1920, 1080),
            (PixelPoint::new(20, -1), 1920, 1080),
            (PixelPoint::new(1920, 500), 1920, 1080),
            (PixelPoint::new(20, 1080), 1920, 1080),
            (PixelPoint::new(20, 500), 0, 1080),
            (PixelPoint::new(20, 500), 1920, 0),
        ] {
            assert!(client.move_pointer(point, width, height).is_err());
        }

        client
            .move_pointer(PixelPoint::new(20, 500), 1920, 1080)
            .expect("acknowledged absolute movement");
        drop(client);
        server.join().expect("join movement mock");
    }


    /// A rejected pointer movement returns the error without replaying input.
    #[test]
    fn pointer_movement_requires_acknowledgement_without_retry() {
        let (client_stream, mut server_stream) = UnixStream::pair().expect("create socket pair");
        server_stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("set mock server timeout");
        let server_reader = server_stream.try_clone().expect("clone mock server");
        let server = thread::spawn(move || {
            let mut reader = BufReader::new(server_reader);
            start_mock_qmp(&mut server_stream, &mut reader);
            let request = read_request(&mut reader);
            assert_eq!(request["execute"], "input-send-event");
            assert_eq!(
                request["arguments"]["events"]
                    .as_array()
                    .expect("events")
                    .len(),
                2
            );
            write_packet(
                &mut server_stream,
                &json!({
                    "error": { "class": "GenericError", "desc": "movement rejected" },
                    "id": request_id(&request)
                }),
            );
            let mut line = String::new();
            assert_eq!(reader.read_line(&mut line).expect("read after error"), 0);
        });
        let mut client = QmpClient::from_connected_stream(Path::new("mock-qmp"), client_stream)
            .expect("connect mock client");
        let error = client
            .move_pointer(PixelPoint::new(20, 500), 1920, 1080)
            .expect_err("movement requires a successful acknowledgement");
        assert!(matches!(error, QmpError::Command(_)));
        drop(client);
        server.join().expect("join rejected movement mock");
    }


    /// Verify PNG command arguments and reject invalid paths before any request.
    #[test]
    fn screendump_uses_png_and_rejects_invalid_paths_before_sending() {
        let (client_stream, mut server_stream) = UnixStream::pair().expect("create socket pair");
        server_stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("set mock server timeout");
        let server_reader = server_stream.try_clone().expect("clone mock server");

        let server = thread::spawn(move || {
            let mut reader = BufReader::new(server_reader);
            start_mock_qmp(&mut server_stream, &mut reader);

            // Invalid calls must not put requests on the wire, so the next
            // packet received here must be the one valid screendump request.
            let request = read_request(&mut reader);
            assert_eq!(request["execute"], "screendump");
            assert_eq!(
                request["arguments"],
                json!({
                    "filename": "/tmp/xsar-frame.png",
                    "format": "png"
                })
            );
            acknowledge(&mut server_stream, &request);
        });

        let mut client = QmpClient::from_connected_stream(Path::new("mock-qmp"), client_stream)
            .expect("connect mock client");
        let relative = client
            .screendump_png(Path::new("frame.png"))
            .expect_err("relative output path must fail");
        assert!(matches!(relative, QmpError::CapturePath(_)));

        let non_utf8 = PathBuf::from(OsString::from_vec(vec![b'/', b't', b'm', b'p', b'/', 0xff]));
        let non_utf8 = client
            .screendump_png(&non_utf8)
            .expect_err("non-UTF-8 output path must fail");
        assert!(matches!(non_utf8, QmpError::CapturePath(_)));

        let nul_path = client
            .screendump_png(Path::new("/tmp/frame\0.png"))
            .expect_err("NUL-containing output path must fail");
        assert!(matches!(nul_path, QmpError::CapturePath(_)));

        client
            .screendump_png(Path::new("/tmp/xsar-frame.png"))
            .expect("send screendump command");
        drop(client);
        server.join().expect("mock QMP server completed");
    }


    /// Verify movement ordering and release delivery before either input acknowledgement.
    #[test]
    fn click_and_key_send_release_before_waiting_for_down_ack() {
        let (client_stream, mut server_stream) = UnixStream::pair().expect("create socket pair");
        server_stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("set mock server timeout");
        let server_reader = server_stream.try_clone().expect("clone mock server");

        let server = thread::spawn(move || {
            let mut reader = BufReader::new(server_reader);
            start_mock_qmp(&mut server_stream, &mut reader);

            let movement = read_request(&mut reader);
            assert_eq!(movement["execute"], "input-send-event");
            assert_eq!(
                movement["arguments"]["events"],
                json!([
                    {
                        "type": "abs",
                        "data": {
                            "axis": "x",
                            "value": pixel_to_qmp_axis(400, 1920).unwrap()
                        }
                    },
                    {
                        "type": "abs",
                        "data": {
                            "axis": "y",
                            "value": pixel_to_qmp_axis(800, 1080).unwrap()
                        }
                    }
                ])
            );

            // Withhold the movement ACK for longer than the settle delay. A
            // client that does not wait for it will reveal a premature down.
            reader
                .get_ref()
                .set_read_timeout(Some(
                    QmpConfig::default().pointer_settle_delay + Duration::from_millis(30),
                ))
                .expect("set movement gate timeout");
            let mut premature = String::new();
            let error = reader
                .read_line(&mut premature)
                .expect_err("button-down arrived before movement ACK");
            assert!(matches!(
                error.kind(),
                ErrorKind::WouldBlock | ErrorKind::TimedOut
            ));
            assert!(premature.is_empty());
            reader
                .get_ref()
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("restore mock server timeout");
            acknowledge(&mut server_stream, &movement);

            let button_down = read_request(&mut reader);
            assert_eq!(first_input_event(&button_down)["type"], "btn");
            assert_eq!(first_input_event(&button_down)["data"]["button"], "left");
            assert_eq!(first_input_event(&button_down)["data"]["down"], true);

            // No response is sent for down. Receiving up proves the client did
            // not block waiting for that ACK.
            let button_up = read_request(&mut reader);
            assert_eq!(first_input_event(&button_up)["type"], "btn");
            assert_eq!(first_input_event(&button_up)["data"]["button"], "left");
            assert_eq!(first_input_event(&button_up)["data"]["down"], false);

            // An asynchronous event and reverse response order must not make
            // the client discard either requested response.
            write_packet(
                &mut server_stream,
                &json!({ "event": "RESET", "timestamp": { "seconds": 0, "microseconds": 0 } }),
            );
            acknowledge(&mut server_stream, &button_up);
            acknowledge(&mut server_stream, &button_down);

            let key_down = read_request(&mut reader);
            assert_eq!(first_input_event(&key_down)["type"], "key");
            assert_eq!(first_input_event(&key_down)["data"]["down"], true);
            assert_eq!(
                first_input_event(&key_down)["data"]["key"],
                json!({ "type": "qcode", "data": "d" })
            );

            let key_up = read_request(&mut reader);
            assert_eq!(first_input_event(&key_up)["type"], "key");
            assert_eq!(first_input_event(&key_up)["data"]["down"], false);
            assert_eq!(
                first_input_event(&key_up)["data"]["key"],
                json!({ "type": "qcode", "data": "d" })
            );
            acknowledge(&mut server_stream, &key_down);
            acknowledge(&mut server_stream, &key_up);
        });

        let mut client = QmpClient::from_connected_stream(Path::new("mock-qmp"), client_stream)
            .expect("connect mock client");
        client
            .click(PixelPoint::new(400, 800), 1920, 1080)
            .expect("send click");
        client.press_key_default("d").expect("send D key");
        drop(client);
        server.join().expect("mock QMP server completed");
    }


    /// Verify causal-error retention and release-only recovery without a second press.
    #[test]
    fn command_failure_releases_on_original_and_fresh_connections_without_retrying_down() {
        let socket_file = SocketFile(std::env::temp_dir().join(format!(
            "xsar-qmp-test-{}-{}.sock",
            process::id(),
            NEXT_SOCKET_ID.fetch_add(1, Ordering::Relaxed)
        )));
        let _ = fs::remove_file(&socket_file.0);
        let listener = UnixListener::bind(&socket_file.0).expect("bind mock QMP socket");
        listener
            .set_nonblocking(true)
            .expect("make mock listener nonblocking");

        let server = thread::spawn(move || {
            let mut first_stream = accept_with_timeout(&listener);
            first_stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("set original connection timeout");
            let first_reader = first_stream.try_clone().expect("clone original stream");
            let mut first_reader = BufReader::new(first_reader);
            start_mock_qmp(&mut first_stream, &mut first_reader);

            let key_down = read_request(&mut first_reader);
            assert_eq!(first_input_event(&key_down)["data"]["down"], true);

            // Still withhold the down response until the release is present on
            // the wire. The action itself must occur exactly once.
            let key_up = read_request(&mut first_reader);
            assert_eq!(first_input_event(&key_up)["data"]["down"], false);
            write_packet(
                &mut first_stream,
                &json!({
                    "error": { "class": "GenericError", "desc": "injected down failure" },
                    "id": request_id(&key_down)
                }),
            );
            // Do not acknowledge up. The client must retain the causal down
            // error and proceed directly to release-only recovery.

            let original_release = read_request(&mut first_reader);
            assert_eq!(first_input_event(&original_release)["data"]["down"], false);

            // recover_release shuts the uncertain stream before connecting a
            // fresh client, so the original peer must now reach EOF.
            let mut trailing = Vec::new();
            first_reader
                .read_to_end(&mut trailing)
                .expect("read original connection EOF");
            assert!(trailing.is_empty());
            drop(first_reader);
            drop(first_stream);

            let mut fresh_stream = accept_with_timeout(&listener);
            fresh_stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("set fresh connection timeout");
            let fresh_reader = fresh_stream.try_clone().expect("clone fresh stream");
            let mut fresh_reader = BufReader::new(fresh_reader);
            start_mock_qmp(&mut fresh_stream, &mut fresh_reader);

            let fresh_release = read_request(&mut fresh_reader);
            assert_eq!(first_input_event(&fresh_release)["type"], "key");
            assert_eq!(first_input_event(&fresh_release)["data"]["down"], false);
            assert_eq!(
                first_input_event(&fresh_release)["data"]["key"],
                json!({ "type": "qcode", "data": "d" })
            );
            acknowledge(&mut fresh_stream, &fresh_release);
        });

        let mut client = QmpClient::connect(&socket_file.0).expect("connect original QMP client");
        let error = client
            .press_key_default("d")
            .expect_err("injected down error must fail the action");
        assert!(matches!(
            error,
            QmpError::Command(ref message) if message == "injected down failure"
        ));

        server.join().expect("mock QMP recovery server completed");
    }
}
