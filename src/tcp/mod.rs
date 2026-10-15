//! Guest screen capture using the versioned, little-endian TCP protocol.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use crate::pixel_format;

/// Shared request, response and checked stride contract for both endpoints.
pub mod protocol;

/// TCP server address (guest runs on this port).
const DEFAULT_SERVER_ADDR: &str = "127.0.0.1:9999";

/// Connection timeout.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Read/write timeout for individual socket operations.
const SOCKET_TIMEOUT: Duration = Duration::from_secs(10);

/// Error type for TCP capture operations.
#[derive(Debug, Clone)]
pub enum TcpCaptureError {
    /// Failed to connect to guest server.
    ConnectionFailed(String),
    /// Socket operation timed out.
    Timeout,
    /// Protocol error (magic mismatch, malformed response).
    ProtocolError(String),
    /// I/O error during read/write.
    IoError(String),
    /// Pixel format conversion failed.
    ConversionError(String),
    /// Response indicates guest-side error.
    GuestError(String),
}

impl std::fmt::Display for TcpCaptureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ConnectionFailed(msg) => write!(f, "TCP connection failed: {}", msg),
            Self::Timeout => write!(f, "TCP operation timed out"),
            Self::ProtocolError(msg) => write!(f, "Protocol error: {}", msg),
            Self::IoError(msg) => write!(f, "I/O error: {}", msg),
            Self::ConversionError(msg) => write!(f, "Pixel conversion failed: {}", msg),
            Self::GuestError(msg) => write!(f, "Guest error: {}", msg),
        }
    }
}

impl std::error::Error for TcpCaptureError {}

/// Represents a single captured frame from guest via TCP.
#[derive(Debug, Clone)]
pub struct TcpCapturedFrame {
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
    /// Tightly packed RGBA bytes per row, widened from the wire's `u16` for indexing.
    pub stride: usize,
    /// Raw RGBA pixels (post-conversion from guest BGRA).
    pub pixels: Vec<u8>,
}

/// Timing breakdown for TCP capture operation.
#[derive(Debug, Clone, Default)]
pub struct TcpCaptureTiming {
    /// Time to establish connection.
    pub connect_ms: f64,
    /// Time to send request and receive response header.
    pub request_ms: f64,
    /// Time to read all pixel data from socket.
    pub pixel_read_ms: f64,
    /// Time to convert BGRA → RGBA.
    pub conversion_ms: f64,
    /// Total time from request start to frame ready.
    pub total_ms: f64,
}

/// Sync TCP client for guest-side screen capture.
/// 
/// Requests one full-screen BGRA capture per connection to the Windows 11
/// guest, matching the server's connection lifetime. Converts BGRA to RGBA
/// after validating the complete frame geometry and bounded payload size.
pub struct ScreenCaptureClient {
    server_addr: String,
    stream: Option<TcpStream>,
}

impl ScreenCaptureClient {
    /// Create a new TCP capture client pointing to the guest server.
    pub fn new(server_addr: Option<String>) -> Self {
        Self {
            server_addr: server_addr.unwrap_or_else(|| DEFAULT_SERVER_ADDR.to_string()),
            stream: None,
        }
    }

    /// Establish or re-establish connection to guest server.
    fn connect(&mut self) -> Result<(), TcpCaptureError> {
        let addr = self
            .server_addr
            .to_socket_addrs()
            .map_err(|e| TcpCaptureError::ConnectionFailed(e.to_string()))?
            .next()
            .ok_or_else(|| {
                TcpCaptureError::ConnectionFailed(format!("Invalid address: {}", self.server_addr))
            })?;

        let stream = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT)
            .map_err(|e| TcpCaptureError::ConnectionFailed(e.to_string()))?;

        stream
            .set_read_timeout(Some(SOCKET_TIMEOUT))
            .map_err(|e| TcpCaptureError::IoError(e.to_string()))?;
        stream
            .set_write_timeout(Some(SOCKET_TIMEOUT))
            .map_err(|e| TcpCaptureError::IoError(e.to_string()))?;

        self.stream = Some(stream);
        Ok(())
    }

    /// Ensure we have an active connection; reconnect if needed.
    fn ensure_connected(&mut self) -> Result<(), TcpCaptureError> {
        if self.stream.is_none() {
            self.connect()?;
        }
        Ok(())
    }

    /// Send a CAPTURE_FULL request and read the response.
    ///
    /// Protocol:
    ///   Request: [magic: u8][version: u8][command: u8][reserved: 2 bytes].
    ///   Response: [width: u32][height: u32][stride: u16][frame_number: u32],
    ///   then top-down BGRA pixels. Integers are little-endian, without padding.
    ///
    /// The version-2 response header is 14 bytes. Stride is `width * 4`, so
    /// 1920 pixels use 7680 bytes per row. A `u16` holds at most 65535 bytes;
    /// the largest packed four-byte row is 65532 bytes (16383 pixels).
    /// Negative or oversized producer strides must be rejected before narrowing;
    /// an unsigned wire field cannot preserve a sender's signed interpretation.
    ///
    /// Returns (frame, timing) on success.
    pub fn capture_full(&mut self) -> Result<(TcpCapturedFrame, TcpCaptureTiming), TcpCaptureError> {
        let mut timing = TcpCaptureTiming::default();
        let start = std::time::Instant::now();

        self.ensure_connected()?;

        let connect_end = start.elapsed();
        timing.connect_ms = connect_end.as_secs_f64() * 1000.0;

        // Send request.
        let request_start = std::time::Instant::now();
        let mut stream = self.stream.take().ok_or_else(|| {
            TcpCaptureError::IoError("Stream was lost during capture".to_string())
        })?;

        stream
            .write_all(&protocol::REQUEST_BYTES)
            .map_err(|e| TcpCaptureError::IoError(format!("Failed to send request: {}", e)))?;

        // Parse the shared 14-byte header before allocating or reading pixels.
        let mut header_bytes = [0u8; protocol::HEADER_LEN];
        stream
            .read_exact(&mut header_bytes)
            .map_err(|e| TcpCaptureError::IoError(format!("Failed to read response header: {}", e)))?;

        let header = protocol::FrameHeader::from_bytes(header_bytes)
            .map_err(|message| TcpCaptureError::ProtocolError(message.to_string()))?;
        let expected_pixel_bytes = header.pixel_bytes();

        // Read pixel data (BGRA from guest).
        let pixel_read_start = std::time::Instant::now();
        let mut bgra_pixels = vec![0u8; expected_pixel_bytes];
        stream
            .read_exact(&mut bgra_pixels)
            .map_err(|e| TcpCaptureError::IoError(format!("Failed to read pixel data: {}", e)))?;

        let pixel_read_end = pixel_read_start.elapsed();
        timing.pixel_read_ms = pixel_read_end.as_secs_f64() * 1000.0;

        // The server closes after one frame; reconnect for the next capture.
        drop(stream);

        let request_end = request_start.elapsed();
        timing.request_ms = (request_end.as_secs_f64() * 1000.0) - timing.pixel_read_ms;

        // Convert BGRA → RGBA in-place (zero-copy).
        let conversion_start = std::time::Instant::now();
        pixel_format::bgra_to_rgba_inplace(&mut bgra_pixels);
        let rgba_pixels = bgra_pixels;

        let conversion_end = conversion_start.elapsed();
        timing.conversion_ms = conversion_end.as_secs_f64() * 1000.0;

        timing.total_ms = start.elapsed().as_secs_f64() * 1000.0;

        Ok((
            TcpCapturedFrame {
                width: header.width(),
                height: header.height(),
                stride: usize::from(header.stride()),
                pixels: rgba_pixels,
            },
            timing,
        ))
    }

    /// Explicitly close the connection (optional; dropped on client drop).
    pub fn disconnect(&mut self) {
        self.stream = None;
    }
}

impl Drop for ScreenCaptureClient {
    fn drop(&mut self) {
        self.disconnect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = TcpCaptureError::ConnectionFailed("connection refused".to_string());
        assert_eq!(err.to_string(), "TCP connection failed: connection refused");

        let err = TcpCaptureError::Timeout;
        assert_eq!(err.to_string(), "TCP operation timed out");
    }

    #[test]
    fn test_client_creation() {
        let client = ScreenCaptureClient::new(None);
        assert_eq!(client.server_addr, DEFAULT_SERVER_ADDR);

        let client = ScreenCaptureClient::new(Some("192.168.1.1:9999".to_string()));
        assert_eq!(client.server_addr, "192.168.1.1:9999");
    }

    #[test]
    fn test_timing_default() {
        let timing = TcpCaptureTiming::default();
        assert_eq!(timing.connect_ms, 0.0);
        assert_eq!(timing.request_ms, 0.0);
        assert_eq!(timing.conversion_ms, 0.0);
        assert_eq!(timing.total_ms, 0.0);
    }

    #[test]
    fn test_captured_frame_structure() {
        let frame = TcpCapturedFrame {
            width: 1920,
            height: 1080,
            stride: 1920 * 4,
            pixels: vec![0u8; 1920 * 1080 * 4],
        };
        assert_eq!(frame.width, 1920);
        assert_eq!(frame.height, 1080);
        assert_eq!(frame.stride, 7680);
        assert_eq!(frame.pixels.len(), 1920 * 1080 * 4);
    }
}
