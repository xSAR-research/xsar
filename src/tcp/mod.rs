//! Bounded, cancellable guest capture using explicit version-2 and version-3 TCP modes.
//!
//! Version 3 transfers raw BGR8 and reconstructs owned RGBA8 pixels with alpha
//! 255. Persistent mode permits one outstanding request, validates identity and
//! full coverage, and discards the entire connection after any capture error.
//! No capture request or game input is replayed within a failed operation.

use crate::parameters::{
    TCP_CANCEL_POLL as CANCEL_POLL, TCP_CONNECT_TIMEOUT as CONNECT_TIMEOUT,
    TCP_SOCKET_TIMEOUT as SOCKET_TIMEOUT,
};
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};

use std::time::{Duration, Instant};


/// Explicit legacy and version-3 request/response definitions.
pub mod protocol;


/// Default numeric guest-forwarding endpoint; DNS resolution is not performed.
const DEFAULT_SERVER_ADDR: &str = "127.0.0.1:9999";


/// Explicit wire and lifetime selection; no protocol fallback is inferred.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CaptureMode {
    /// Protocol 2 BGRA8 with one frame per connection.
    LegacyV2,
    /// Protocol 3 BGR8 with one frame per connection, for controlled comparisons.
    BgrV3,
    /// Protocol 3 BGR8 with a validated retained connection.
    #[default]
    PersistentBgrV3,
}


/// Error type for TCP capture operations; no variant contains partial pixels.
#[derive(Debug, Clone)]
pub enum TcpCaptureError {
    /// Connection establishment or endpoint validation failed.
    ConnectionFailed(String),
    /// An absolute operation deadline elapsed.
    Timeout,
    /// Cooperative STOP/cancellation was observed.
    Cancelled,
    /// Invalid version, geometry, identity, framing or response metadata.
    ProtocolError(String),
    /// Socket, allocation or other bounded I/O failure.
    IoError(String),
    /// A checked pixel conversion could not satisfy the destination layout.
    ConversionError(String),
    /// The peer returned a recognised zero-payload failure status.
    GuestError(String),
}


impl std::fmt::Display for TcpCaptureError {
    /// Format a bounded diagnostic suitable for the caller's failure log.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ConnectionFailed(msg) => write!(f, "TCP connection failed: {msg}"),
            Self::Timeout => write!(f, "TCP operation timed out"),
            Self::Cancelled => write!(f, "TCP capture cancelled"),
            Self::ProtocolError(msg) => write!(f, "Protocol error: {msg}"),
            Self::IoError(msg) => write!(f, "I/O error: {msg}"),
            Self::ConversionError(msg) => write!(f, "Pixel conversion failed: {msg}"),
            Self::GuestError(msg) => write!(f, "Guest error: {msg}"),
        }
    }
}


impl std::error::Error for TcpCaptureError {}


/// Owned complete native frame; failed operations never publish this value.
#[derive(Debug, Clone)]
pub struct TcpCapturedFrame {
    /// Complete native display width in pixels.
    pub width: u32,
    /// Complete native display height in pixels.
    pub height: u32,
    /// Tightly packed RGBA8 bytes per row.
    pub stride: usize,
    /// Top-down R, G, B, 255 bytes for every real pixel.
    pub pixels: Vec<u8>,
}


/// Correlation and byte counts from the validated response; no wall-clock subtraction.
#[derive(Debug, Clone, Default)]
pub struct TcpCaptureMetadata {
    /// Explicit protocol version, 2 or 3.
    pub protocol_version: u8,
    /// Server session identity; zero when legacy protocol provides none.
    pub session_id: u64,
    /// Client request identity; zero when legacy protocol provides none.
    pub request_id: u64,
    /// Server capture identity, widened from `u32` in legacy mode.
    pub frame_id: u64,
    /// Display generation; zero when legacy protocol provides none.
    pub display_generation: u64,
    /// Actual request wire length, excluding TCP overhead.
    pub request_bytes: usize,
    /// Actual response header length, excluding TCP overhead.
    pub response_header_bytes: usize,
    /// Uncompressed wire pixel length, before RGBA expansion.
    pub decoded_bytes: usize,
    /// Actual transmitted pixel payload; raw modes equal decoded bytes.
    pub payload_bytes: usize,
    /// Whether this operation used a connection retained from a prior valid response.
    pub reused_connection: bool,
}


/// Monotonic timings in milliseconds; `request_ms` aggregates write and header fields.
#[derive(Debug, Clone, Default)]
pub struct TcpCaptureTiming {
    /// Connection attempt duration; exactly zero when the connection was reused.
    pub connect_ms: f64,
    /// Request write plus header wait, excluding allocation and pixel reads.
    pub request_ms: f64,
    /// Payload socket reads only; allocation is recorded separately.
    pub pixel_read_ms: f64,
    /// Channel ordering and opaque alpha insertion into the final destination.
    pub conversion_ms: f64,
    /// Entire API call through complete validated RGBA pixels; includes bookkeeping.
    pub total_ms: f64,
    /// Request write only, ending when all request bytes are accepted by the socket.
    pub request_write_ms: f64,
    /// Response header wait/read, starting after the request write finishes.
    pub header_wait_ms: f64,
    /// Wire-buffer allocation and zero initialisation, before payload read.
    pub allocation_ms: f64,
    /// Final RGBA canvas allocation and zero initialisation; zero for in-place v2.
    pub canvas_prepare_ms: f64,
    /// Validated identifiers, wire byte counts and connection reuse.
    pub metadata: TcpCaptureMetadata,
}


/// Synchronous single-request client with bounded full-frame allocation and socket work.
///
/// Endpoints must be numeric `SocketAddr` strings so name resolution cannot escape
/// the connection deadline. A connection attempt is bounded by five seconds and
/// checks cancellation before and after that blocking attempt. Established socket
/// I/O polls cancellation approximately every 100 ms with independent absolute
/// ten-second request/header/payload deadlines. No error triggers automatic replay.
pub struct ScreenCaptureClient {
    /// Numeric IP address and port selected by the application.
    server_addr: String,
    /// Explicit protocol and connection-lifetime contract.
    mode: CaptureMode,
    /// Retained only after complete successful persistent responses.
    stream: Option<TcpStream>,
    /// Session established by the immediately previous valid response.
    session_id: u64,
    /// Nonzero identity to use for the next request on this connection.
    next_request_id: u64,
    /// Expected stable display generation on the retained connection.
    display_generation: u64,
    /// Latest successfully received frame identity on this connection.
    last_frame_id: u64,
    /// Established display dimensions, used to reject silent display changes.
    display_dimensions: Option<(u32, u32)>,
    /// Absolute per-stage limit; private override supports fast deadline tests.
    socket_timeout: Duration,
}


impl ScreenCaptureClient {
    /// Create the default persistent version-3 client; compatibility is explicitly selected.
    pub fn new(server_addr: Option<String>) -> Self {
        Self::with_mode(server_addr, CaptureMode::PersistentBgrV3)
    }


    /// Select an explicit protocol and lifetime without fallback or automatic probing.
    pub fn with_mode(server_addr: Option<String>, mode: CaptureMode) -> Self {
        Self {
            server_addr: server_addr.unwrap_or_else(|| DEFAULT_SERVER_ADDR.to_string()),
            mode,
            stream: None,
            session_id: 0,
            next_request_id: 1,
            display_generation: 0,
            last_frame_id: 0,
            display_dimensions: None,
            socket_timeout: SOCKET_TIMEOUT,
        }
    }


    /// Acquire one fresh full frame with no external cancellation request.
    pub fn capture_full(
        &mut self,
    ) -> Result<(TcpCapturedFrame, TcpCaptureTiming), TcpCaptureError> {
        self.capture_full_cancellable(&AtomicBool::new(false))
    }


    /// Acquire one complete frame; STOP or any error discards connection and identity state.
    ///
    /// The caller owns the cancellation flag and may raise it from another thread.
    /// Returned bytes are exclusively owned and cannot be overwritten by another capture.
    pub fn capture_full_cancellable(
        &mut self,
        cancelled: &AtomicBool,
    ) -> Result<(TcpCapturedFrame, TcpCaptureTiming), TcpCaptureError> {
        let result = self.capture_once(cancelled);
        if result.is_err() {
            self.disconnect();
        }
        result
    }


    /// Perform one request, replacing a retained socket only on pre-request clean EOF.
    ///
    /// Unfinished resources stay local until validation; failures never replay a request.
    fn capture_once(
        &mut self,
        cancelled: &AtomicBool,
    ) -> Result<(TcpCapturedFrame, TcpCaptureTiming), TcpCaptureError> {
        let start = Instant::now();
        check_cancelled(cancelled)?;
        if let Some(stream) = self.stream.as_ref() {
            let peer_open = reject_unsolicited_data(stream)?;
            if !peer_open {
                self.disconnect();
            }
        }
        let reused_connection = self.stream.is_some();
        let mut timing = TcpCaptureTiming::default();
        if self.stream.is_none() {
            let connect_start = Instant::now();
            self.connect(cancelled)?;
            timing.connect_ms = millis(connect_start.elapsed());
        }
        let mut stream = self
            .stream
            .take()
            .ok_or_else(|| TcpCaptureError::IoError("missing capture socket".to_string()))?;
        let frame = match self.mode {
            CaptureMode::LegacyV2 => self.capture_legacy(&mut stream, cancelled, &mut timing)?,
            CaptureMode::BgrV3 | CaptureMode::PersistentBgrV3 => {
                self.capture_v3(&mut stream, cancelled, &mut timing)?
            }
        };
        check_cancelled(cancelled)?;
        self.finish_response_connection(stream)?;
        timing.metadata.reused_connection = reused_connection;
        timing.request_ms = timing.request_write_ms + timing.header_wait_ms;
        timing.total_ms = millis(start.elapsed());
        Ok((frame, timing))
    }


    /// Retain only a complete response's still-open socket; clean EOF preserves the valid frame.
    fn finish_response_connection(&mut self, stream: TcpStream) -> Result<(), TcpCaptureError> {
        let peer_open = reject_unsolicited_data(&stream)?;
        if self.mode == CaptureMode::PersistentBgrV3 && peer_open {
            self.stream = Some(stream);
        } else {
            self.reset_identity();
        }
        Ok(())
    }


    /// Establish one bounded numeric-address connection; never perform unbounded DNS lookup.
    fn connect(&mut self, cancelled: &AtomicBool) -> Result<(), TcpCaptureError> {
        check_cancelled(cancelled)?;
        let addr = self.server_addr.parse::<SocketAddr>().map_err(|_| {
            TcpCaptureError::ConnectionFailed(
                "endpoint must be a numeric IP address and port".to_string(),
            )
        })?;
        let result = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT);
        check_cancelled(cancelled)?;
        let stream = result.map_err(|error| {
            if retryable_timeout(&error) {
                TcpCaptureError::Timeout
            } else {
                TcpCaptureError::ConnectionFailed(error.to_string())
            }
        })?;
        self.stream = Some(stream);
        Ok(())
    }


    /// Send the explicitly selected legacy request and validate the original 14-byte header.
    fn capture_legacy(
        &self,
        stream: &mut TcpStream,
        cancelled: &AtomicBool,
        timing: &mut TcpCaptureTiming,
    ) -> Result<TcpCapturedFrame, TcpCaptureError> {
        let write_start = Instant::now();
        write_bounded(
            stream,
            &protocol::REQUEST_BYTES,
            cancelled,
            self.socket_timeout,
        )?;
        timing.request_write_ms = millis(write_start.elapsed());
        let header_start = Instant::now();
        let mut bytes = [0; protocol::HEADER_LEN];
        read_bounded(stream, &mut bytes, cancelled, self.socket_timeout)?;
        timing.header_wait_ms = millis(header_start.elapsed());
        let header = protocol::FrameHeader::from_bytes(bytes).map_err(protocol_error)?;
        let allocation_start = Instant::now();
        let mut pixels = allocate_zeroed(header.pixel_bytes())?;
        timing.allocation_ms = millis(allocation_start.elapsed());
        let payload_start = Instant::now();
        read_bounded(stream, &mut pixels, cancelled, self.socket_timeout)?;
        timing.pixel_read_ms = millis(payload_start.elapsed());
        let conversion_start = Instant::now();
        for (index, pixel) in pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            if index.is_multiple_of(4096) {
                check_cancelled(cancelled)?;
            }
            pixel.swap(0, 2);
            pixel[3] = 255;
        }
        timing.conversion_ms = millis(conversion_start.elapsed());
        timing.metadata = TcpCaptureMetadata {
            protocol_version: protocol::PROTOCOL_VERSION,
            frame_id: u64::from(header.frame_number()),
            request_bytes: protocol::REQUEST_BYTES.len(),
            response_header_bytes: protocol::HEADER_LEN,
            decoded_bytes: header.pixel_bytes(),
            payload_bytes: header.pixel_bytes(),
            ..TcpCaptureMetadata::default()
        };
        Ok(TcpCapturedFrame {
            width: header.width(),
            height: header.height(),
            stride: usize::from(header.stride()),
            pixels,
        })
    }


    /// Request raw BGR8, reject incompatible metadata, then reconstruct one owned RGBA frame.
    fn capture_v3(
        &mut self,
        stream: &mut TcpStream,
        cancelled: &AtomicBool,
        timing: &mut TcpCaptureTiming,
    ) -> Result<TcpCapturedFrame, TcpCaptureError> {
        use protocol::v3;
        let request = v3::Request::new(
            self.session_id,
            self.next_request_id,
            self.display_generation,
        )
        .map_err(protocol_error)?;
        let write_start = Instant::now();
        write_bounded(stream, &request.to_bytes(), cancelled, self.socket_timeout)?;
        timing.request_write_ms = millis(write_start.elapsed());
        let header_start = Instant::now();
        let mut bytes = [0; v3::HEADER_LEN];
        read_bounded(stream, &mut bytes, cancelled, self.socket_timeout)?;
        timing.header_wait_ms = millis(header_start.elapsed());
        let header = v3::ResponseHeader::from_bytes(bytes).map_err(protocol_error)?;
        self.validate_response(&header, request.request_id())?;
        if header.status() != v3::Status::Ok {
            return Err(TcpCaptureError::GuestError(format!(
                "{:?}",
                header.status()
            )));
        }
        let next_request_id = self
            .next_request_id
            .checked_add(1)
            .ok_or_else(|| protocol_error("version-3 request identity exhausted"))?;
        let allocation_start = Instant::now();
        let mut wire = allocate_zeroed(header.pixel_bytes())?;
        timing.allocation_ms = millis(allocation_start.elapsed());
        let payload_start = Instant::now();
        read_bounded(stream, &mut wire, cancelled, self.socket_timeout)?;
        timing.pixel_read_ms = millis(payload_start.elapsed());
        check_cancelled(cancelled)?;
        let canvas_start = Instant::now();
        let mut pixels = allocate_zeroed(header.rgba_bytes())?;
        timing.canvas_prepare_ms = millis(canvas_start.elapsed());
        let conversion_start = Instant::now();
        bgr_to_rgba(&wire, &mut pixels, cancelled)?;
        timing.conversion_ms = millis(conversion_start.elapsed());
        self.session_id = header.session_id();
        self.display_generation = header.display_generation();
        self.last_frame_id = header.frame_id();
        self.display_dimensions = Some((header.width(), header.height()));
        self.next_request_id = next_request_id;
        timing.metadata = TcpCaptureMetadata {
            protocol_version: v3::PROTOCOL_VERSION,
            session_id: header.session_id(),
            request_id: header.request_id(),
            frame_id: header.frame_id(),
            display_generation: header.display_generation(),
            request_bytes: v3::REQUEST_LEN,
            response_header_bytes: v3::HEADER_LEN,
            decoded_bytes: header.pixel_bytes(),
            payload_bytes: header.pixel_bytes(),
            reused_connection: false,
        };
        Ok(TcpCapturedFrame {
            width: header.width(),
            height: header.height(),
            stride: header.rgba_stride(),
            pixels,
        })
    }


    /// Reject stale/cross-session responses and display changes before reading pixel bytes.
    fn validate_response(
        &self,
        header: &protocol::v3::ResponseHeader,
        request_id: u64,
    ) -> Result<(), TcpCaptureError> {
        if header.request_id() != request_id
            || (self.session_id != 0 && header.session_id() != self.session_id)
        {
            return Err(protocol_error(
                "version-3 response session or request identity mismatch",
            ));
        }
        if header.status() == protocol::v3::Status::Ok {
            if header.frame_id() <= self.last_frame_id {
                return Err(protocol_error(
                    "version-3 frame identity is stale or repeated",
                ));
            }
            if self.display_generation != 0
                && header.display_generation() != self.display_generation
            {
                return Err(protocol_error("version-3 display generation changed"));
            }
            if self
                .display_dimensions
                .is_some_and(|dimensions| dimensions != (header.width(), header.height()))
            {
                return Err(protocol_error(
                    "version-3 native display dimensions changed",
                ));
            }
        }
        Ok(())
    }


    /// Forget all connection-local correlation after disconnect or failed acquisition.
    fn reset_identity(&mut self) {
        self.session_id = 0;
        self.next_request_id = 1;
        self.display_generation = 0;
        self.last_frame_id = 0;
        self.display_dimensions = None;
    }


    /// Close the retained socket and clear session/frame/display state.
    pub fn disconnect(&mut self) {
        self.stream = None;
        self.reset_identity();
    }
}


impl Drop for ScreenCaptureClient {
    /// Close any retained connection; no input or automatic reconnection occurs.
    fn drop(&mut self) {
        self.disconnect();
    }
}


/// Convert a validated tight BGR slice directly into the exclusively owned RGBA destination.
fn bgr_to_rgba(
    wire: &[u8],
    pixels: &mut [u8],
    cancelled: &AtomicBool,
) -> Result<(), TcpCaptureError> {
    if !wire.len().is_multiple_of(3)
        || !pixels.len().is_multiple_of(4)
        || wire.len() / 3 != pixels.len() / 4
    {
        return Err(TcpCaptureError::ConversionError(
            "BGR/RGBA lengths disagree".to_string(),
        ));
    }
    for (index, (source, destination)) in wire
        .as_chunks::<3>()
        .0
        .iter()
        .zip(pixels.as_chunks_mut::<4>().0.iter_mut())
        .enumerate()
    {
        if index.is_multiple_of(4096) {
            check_cancelled(cancelled)?;
        }
        destination.copy_from_slice(&[source[2], source[1], source[0], 255]);
    }
    Ok(())
}


/// Allocate and initialise only a previously validated bounded byte count.
fn allocate_zeroed(length: usize) -> Result<Vec<u8>, TcpCaptureError> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(length)
        .map_err(|_| TcpCaptureError::IoError("bounded frame allocation failed".to_string()))?;
    bytes.resize(length, 0);
    Ok(bytes)
}


/// Read exactly one bounded record/payload while preserving partial progress across short polls.
fn read_bounded(
    stream: &mut TcpStream,
    mut bytes: &mut [u8],
    cancelled: &AtomicBool,
    timeout: Duration,
) -> Result<(), TcpCaptureError> {
    let deadline = Instant::now() + timeout;
    while !bytes.is_empty() {
        let remaining = poll_remaining(cancelled, deadline)?;
        stream.set_read_timeout(Some(remaining)).map_err(io_error)?;
        match stream.read(bytes) {
            Ok(0) => {
                return Err(TcpCaptureError::IoError(
                    "truncated capture response".to_string(),
                ));
            }
            Ok(count) => {
                bytes = &mut bytes[count..];
            }
            Err(error)
                if retryable_timeout(&error) || error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(io_error(error)),
        }
    }
    poll_remaining(cancelled, deadline)?;
    Ok(())
}


/// Write exactly one request under an absolute deadline; partial writes are never replayed.
fn write_bounded(
    stream: &mut TcpStream,
    mut bytes: &[u8],
    cancelled: &AtomicBool,
    timeout: Duration,
) -> Result<(), TcpCaptureError> {
    let deadline = Instant::now() + timeout;
    while !bytes.is_empty() {
        let remaining = poll_remaining(cancelled, deadline)?;
        stream
            .set_write_timeout(Some(remaining))
            .map_err(io_error)?;
        match stream.write(bytes) {
            Ok(0) => {
                return Err(TcpCaptureError::IoError(
                    "capture request write made no progress".to_string(),
                ));
            }
            Ok(count) => {
                bytes = &bytes[count..];
            }
            Err(error)
                if retryable_timeout(&error) || error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(io_error(error)),
        }
    }
    poll_remaining(cancelled, deadline)?;
    Ok(())
}


/// Reject unsolicited bytes and report an open peer; clean EOF returns false.
fn reject_unsolicited_data(stream: &TcpStream) -> Result<bool, TcpCaptureError> {
    stream.set_nonblocking(true).map_err(io_error)?;
    let mut byte = [0];
    let result = stream.peek(&mut byte);
    stream.set_nonblocking(false).map_err(io_error)?;
    match result {
        Ok(0) => Ok(false), // A complete final response remains valid even if the peer closes.
        Ok(_) => Err(protocol_error(
            "unsolicited or trailing capture response bytes",
        )),
        Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(true),
        Err(error) => Err(io_error(error)),
    }
}


/// Observe cooperative cancellation before each bounded blocking interval.
fn check_cancelled(cancelled: &AtomicBool) -> Result<(), TcpCaptureError> {
    if cancelled.load(Ordering::Acquire) {
        Err(TcpCaptureError::Cancelled)
    } else {
        Ok(())
    }
}


/// Return at most one poll interval, never extending the absolute operation deadline.
fn poll_remaining(cancelled: &AtomicBool, deadline: Instant) -> Result<Duration, TcpCaptureError> {
    check_cancelled(cancelled)?;
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .ok_or(TcpCaptureError::Timeout)?;
    if remaining.is_zero() {
        return Err(TcpCaptureError::Timeout);
    }
    Ok(remaining.min(CANCEL_POLL))
}


/// Recognise both Unix and Windows socket timeout results.
fn retryable_timeout(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
    )
}


/// Convert a local socket error without exposing partial response storage.
fn io_error(error: io::Error) -> TcpCaptureError {
    TcpCaptureError::IoError(error.to_string())
}


/// Preserve checked protocol diagnostics under the public capture error type.
fn protocol_error(message: &str) -> TcpCaptureError {
    TcpCaptureError::ProtocolError(message.to_string())
}


/// Convert a local monotonic duration to logged milliseconds.
fn millis(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}


#[cfg(test)]
mod tests;
