use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use crate::pixel_format;

/// TCP protocol magic constant for request/response framing.
const PROTOCOL_MAGIC: u32 = 0xDEADBEEF;

/// Request type: full-screen capture.
const REQUEST_CAPTURE_FULL: u8 = 0x01;

/// Maximum PNG size threshold (safety limit, unused here but kept for parity with QMP module).
const SNAPSHOT_MAX_BYTES: u64 = 8 * 1024 * 1024; // 8 MiB

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
    /// Stride (bytes per row, tightly packed for RGBA).
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
/// Maintains a persistent connection to the Windows 11 guest TCP server
/// and requests full-screen BGRA captures. Handles protocol framing,
/// connection retry, and BGRA→RGBA pixel conversion.
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
    ///   Request:  [magic: u32][type: u8] (5 bytes)
    ///   Response: [magic: u32][width: u32][height: u32][stride: u32][pixel_data: &[u8]]
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

        let request = [
            PROTOCOL_MAGIC.to_le_bytes().as_ref(),
            &[REQUEST_CAPTURE_FULL],
        ]
        .concat();

        stream
            .write_all(&request)
            .map_err(|e| TcpCaptureError::IoError(format!("Failed to send request: {}", e)))?;

        // Read response header: magic + width + height + stride.
        let mut header = [0u8; 16];
        stream
            .read_exact(&mut header)
            .map_err(|e| TcpCaptureError::IoError(format!("Failed to read response header: {}", e)))?;

        let magic = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
        if magic != PROTOCOL_MAGIC {
            return Err(TcpCaptureError::ProtocolError(
                format!("Magic mismatch: expected {:#x}, got {:#x}", PROTOCOL_MAGIC, magic),
            ));
        }

        let width = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
        let height = u32::from_le_bytes([header[8], header[9], header[10], header[11]]);
        let stride = u32::from_le_bytes([header[12], header[13], header[14], header[15]]);

        let stride_usize = stride as usize;
        let expected_pixel_bytes = stride_usize * (height as usize);

        if expected_pixel_bytes == 0 || expected_pixel_bytes > SNAPSHOT_MAX_BYTES as usize {
            return Err(TcpCaptureError::ProtocolError(format!(
                "Invalid pixel buffer size: {} bytes (max {})",
                expected_pixel_bytes, SNAPSHOT_MAX_BYTES
            )));
        }

        // Read pixel data (BGRA from guest).
        let pixel_read_start = std::time::Instant::now();
        let mut bgra_pixels = vec![0u8; expected_pixel_bytes];
        stream
            .read_exact(&mut bgra_pixels)
            .map_err(|e| TcpCaptureError::IoError(format!("Failed to read pixel data: {}", e)))?;

        let pixel_read_end = pixel_read_start.elapsed();
        timing.pixel_read_ms = pixel_read_end.as_secs_f64() * 1000.0;

        // Restore stream for next operation.
        self.stream = Some(stream);

        let request_end = request_start.elapsed();
        timing.request_ms = (request_end.as_secs_f64() * 1000.0) - timing.pixel_read_ms;

        if !expected_pixel_bytes.is_multiple_of(4) {
            return Err(TcpCaptureError::ProtocolError(format!(
                "Invalid pixel buffer size not a multiple of 4 bytes: {}",
                expected_pixel_bytes
            )));
        }

        // Convert BGRA → RGBA in-place (zero-copy).
        let conversion_start = std::time::Instant::now();
        pixel_format::bgra_to_rgba_inplace(&mut bgra_pixels);
        let rgba_pixels = bgra_pixels;

        let conversion_end = conversion_start.elapsed();
        timing.conversion_ms = conversion_end.as_secs_f64() * 1000.0;

        timing.total_ms = start.elapsed().as_secs_f64() * 1000.0;

        Ok((
            TcpCapturedFrame {
                width,
                height,
                stride: stride_usize,
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
