//! Fixed-size little-endian capture protocol version 3.
//!
//! One request authorises one fresh, full-display capture. Success carries raw,
//! tightly packed top-down BGR8 pixels; the receiving application reconstructs
//! RGBA8 with alpha 255. No struct memory, compression or regional data is sent.
//! Error headers carry a bounded status and no pixels. All decoded layouts fit
//! the independent 8 MiB canonical RGBA allocation bound.


/// Protocol marker; unlike version 2, the version is separately identified.
pub const MAGIC: [u8; 4] = *b"XSCP";


/// Explicit wire version.
pub const PROTOCOL_VERSION: u8 = 3;


/// Full-display capture command.
pub const COMMAND_CAPTURE_FULL: u8 = 1;


/// Request wire size, excluding any transport framing.
pub const REQUEST_LEN: usize = 32;


/// Response header wire size, excluding pixel bytes.
pub const HEADER_LEN: usize = 80;


/// Tightly packed B, G, R bytes per real pixel.
pub const PIXEL_FORMAT_BGR8: u8 = 1;


/// Rows appear from the top of the display downward.
pub const ORIENTATION_TOP_DOWN: u8 = 1;


/// Raw bytes; compressed data is not supported in this version's initial slice.
pub const COMPRESSION_RAW: u8 = 0;


/// Canonical host RGBA storage limit, including the inserted alpha byte.
pub const MAX_RGBA_BYTES: usize = 8 * 1024 * 1024;


/// Bounded server result; unknown status bytes are never treated as success.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Status {
    /// Complete fresh capture follows the header.
    Ok = 0,
    /// Connected-client, queue or output-buffer capacity was exhausted.
    Busy = 1,
    /// The request was malformed or its session/request identity was invalid.
    InvalidRequest = 2,
    /// Native acquisition or readback failed.
    CaptureFailed = 3,
    /// The request's queue/capture deadline elapsed.
    DeadlineExceeded = 4,
    /// Display identity, generation or dimensions cannot satisfy the request.
    IncompatibleDisplay = 5,
    /// The server is shutting down and will not execute the request.
    ShuttingDown = 6,
}


impl Status {
    /// Decode only known status values.
    pub fn from_byte(value: u8) -> Result<Self, &'static str> {
        match value {
            0 => Ok(Self::Ok),
            1 => Ok(Self::Busy),
            2 => Ok(Self::InvalidRequest),
            3 => Ok(Self::CaptureFailed),
            4 => Ok(Self::DeadlineExceeded),
            5 => Ok(Self::IncompatibleDisplay),
            6 => Ok(Self::ShuttingDown),
            _ => Err("unknown version-3 response status"),
        }
    }
}


/// Checked full-display request; zero session/generation denotes a new session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Request {
    /// Session assigned by the server, or zero for the first request.
    session_id: u64,
    /// Nonzero, increasing client request identity.
    request_id: u64,
    /// Server display generation, or zero for the initial wildcard.
    display_generation: u64,
}


impl Request {
    /// Construct a full request; session and generation must both be initial or known.
    pub fn new(
        session_id: u64,
        request_id: u64,
        display_generation: u64,
    ) -> Result<Self, &'static str> {
        if request_id == 0 {
            return Err("version-3 request identity must be nonzero");
        }
        if (session_id == 0) != (display_generation == 0) {
            return Err("version-3 session and display generation must both be initial or known");
        }
        Ok(Self {
            session_id,
            request_id,
            display_generation,
        })
    }


    /// Decode explicit fields, rejecting unsupported commands and reserved bytes.
    pub fn from_bytes(bytes: [u8; REQUEST_LEN]) -> Result<Self, &'static str> {
        if bytes[0..4] != MAGIC || bytes[4] != PROTOCOL_VERSION {
            return Err("unsupported version-3 request magic or version");
        }
        if bytes[5] != COMMAND_CAPTURE_FULL || bytes[6..8] != [0, 0] {
            return Err("unsupported version-3 request command or reserved bytes");
        }
        Self::new(
            read_u64(&bytes, 8),
            read_u64(&bytes, 16),
            read_u64(&bytes, 24),
        )
    }


    /// Encode exactly 32 bytes without Rust structure padding.
    pub fn to_bytes(&self) -> [u8; REQUEST_LEN] {
        let mut bytes = [0; REQUEST_LEN];
        bytes[0..4].copy_from_slice(&MAGIC);
        bytes[4] = PROTOCOL_VERSION;
        bytes[5] = COMMAND_CAPTURE_FULL;
        bytes[8..16].copy_from_slice(&self.session_id.to_le_bytes());
        bytes[16..24].copy_from_slice(&self.request_id.to_le_bytes());
        bytes[24..32].copy_from_slice(&self.display_generation.to_le_bytes());
        bytes
    }


    /// Server session identity, or zero before the first valid response.
    pub fn session_id(&self) -> u64 {
        self.session_id
    }


    /// Nonzero client request identity.
    pub fn request_id(&self) -> u64 {
        self.request_id
    }


    /// Expected display generation, or zero for the first response.
    pub fn display_generation(&self) -> u64 {
        self.display_generation
    }
}


/// Validated full-frame success or canonical zero-payload error response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResponseHeader {
    /// Bounded result code.
    status: Status,
    /// Server session associated with the request.
    session_id: u64,
    /// Echoed client request identity.
    request_id: u64,
    /// Fresh server capture identity, nonzero on success.
    frame_id: u64,
    /// Display generation, nonzero on success.
    display_generation: u64,
    /// Full native display width; zero for errors.
    width: u32,
    /// Full native display height; zero for errors.
    height: u32,
    /// BGR8 row length; zero for errors.
    stride: u32,
    /// Checked raw BGR8 pixel length; zero for errors.
    pixel_bytes: usize,
}


impl ResponseHeader {
    /// Construct a complete native BGR8 success within the host RGBA allocation cap.
    pub fn success(
        session_id: u64,
        request_id: u64,
        frame_id: u64,
        display_generation: u64,
        width: u32,
        height: u32,
    ) -> Result<Self, &'static str> {
        if session_id == 0 || request_id == 0 || frame_id == 0 || display_generation == 0 {
            return Err("version-3 success identities must be nonzero");
        }
        if width == 0 || height == 0 {
            return Err("version-3 display dimensions must be positive");
        }
        let rgba_stride = width
            .checked_mul(4)
            .ok_or("version-3 RGBA stride overflows")?;
        let rgba_bytes = u64::from(rgba_stride) * u64::from(height);
        if rgba_bytes > MAX_RGBA_BYTES as u64 {
            return Err("version-3 canonical RGBA buffer exceeds 8 MiB limit");
        }
        let stride = width
            .checked_mul(3)
            .ok_or("version-3 BGR stride overflows")?;
        let pixel_bytes = usize::try_from(u64::from(stride) * u64::from(height))
            .map_err(|_| "version-3 pixel buffer exceeds host address space")?;
        Ok(Self {
            status: Status::Ok,
            session_id,
            request_id,
            frame_id,
            display_generation,
            width,
            height,
            stride,
            pixel_bytes,
        })
    }


    /// Construct a bounded error with no format, geometry, frame identity or payload.
    pub fn error(status: Status, session_id: u64, request_id: u64) -> Result<Self, &'static str> {
        if status == Status::Ok || request_id == 0 {
            return Err("version-3 error requires a failure status and nonzero request identity");
        }
        Ok(Self {
            status,
            session_id,
            request_id,
            frame_id: 0,
            display_generation: 0,
            width: 0,
            height: 0,
            stride: 0,
            pixel_bytes: 0,
        })
    }


    /// Decode all explicit fields and validate format, full coverage and lengths before allocation.
    pub fn from_bytes(bytes: [u8; HEADER_LEN]) -> Result<Self, &'static str> {
        if bytes[0..4] != MAGIC || bytes[4] != PROTOCOL_VERSION {
            return Err("unsupported version-3 response magic or version");
        }
        if bytes[76] != COMPRESSION_RAW
            || bytes[77] != COMMAND_CAPTURE_FULL
            || bytes[78..80] != [0, 0]
        {
            return Err("unsupported version-3 compression, command or reserved bytes");
        }
        let status = Status::from_byte(bytes[5])?;
        if status != Status::Ok {
            if bytes[6..8] != [0, 0] || bytes[24..76].iter().any(|byte| *byte != 0) {
                return Err("version-3 error response must have zero frame metadata and payload");
            }
            return Self::error(status, read_u64(&bytes, 8), read_u64(&bytes, 16));
        }
        if bytes[6] != PIXEL_FORMAT_BGR8 || bytes[7] != ORIENTATION_TOP_DOWN {
            return Err("unsupported version-3 pixel format or orientation");
        }
        let header = Self::success(
            read_u64(&bytes, 8),
            read_u64(&bytes, 16),
            read_u64(&bytes, 24),
            read_u64(&bytes, 32),
            read_u32(&bytes, 40),
            read_u32(&bytes, 44),
        )?;
        if read_u32(&bytes, 48) != 0
            || read_u32(&bytes, 52) != 0
            || read_u32(&bytes, 56) != header.width
            || read_u32(&bytes, 60) != header.height
        {
            return Err("version-3 full capture must cover the complete native display");
        }
        if read_u32(&bytes, 64) != header.stride
            || u64::from(read_u32(&bytes, 68)) != header.pixel_bytes as u64
            || read_u32(&bytes, 72) != read_u32(&bytes, 68)
        {
            return Err("version-3 BGR stride or raw payload lengths are inconsistent");
        }
        Ok(header)
    }


    /// Encode exactly 80 bytes; full bounds and raw lengths are unambiguous.
    pub fn to_bytes(&self) -> [u8; HEADER_LEN] {
        let mut bytes = [0; HEADER_LEN];
        bytes[0..4].copy_from_slice(&MAGIC);
        bytes[4] = PROTOCOL_VERSION;
        bytes[5] = self.status as u8;
        bytes[8..16].copy_from_slice(&self.session_id.to_le_bytes());
        bytes[16..24].copy_from_slice(&self.request_id.to_le_bytes());
        bytes[77] = COMMAND_CAPTURE_FULL;
        if self.status == Status::Ok {
            bytes[6] = PIXEL_FORMAT_BGR8;
            bytes[7] = ORIENTATION_TOP_DOWN;
            bytes[24..32].copy_from_slice(&self.frame_id.to_le_bytes());
            bytes[32..40].copy_from_slice(&self.display_generation.to_le_bytes());
            bytes[40..44].copy_from_slice(&self.width.to_le_bytes());
            bytes[44..48].copy_from_slice(&self.height.to_le_bytes());
            bytes[56..60].copy_from_slice(&self.width.to_le_bytes());
            bytes[60..64].copy_from_slice(&self.height.to_le_bytes());
            bytes[64..68].copy_from_slice(&self.stride.to_le_bytes());
            let length = self.pixel_bytes as u32; // Validated below the 8 MiB RGBA cap.
            bytes[68..72].copy_from_slice(&length.to_le_bytes());
            bytes[72..76].copy_from_slice(&length.to_le_bytes());
        }
        bytes
    }


    /// Bounded result code; only `Ok` carries pixels.
    pub fn status(&self) -> Status {
        self.status
    }


    /// Server-assigned session identity.
    pub fn session_id(&self) -> u64 {
        self.session_id
    }


    /// Echoed nonzero request identity.
    pub fn request_id(&self) -> u64 {
        self.request_id
    }


    /// Fresh frame identity, or zero for an error.
    pub fn frame_id(&self) -> u64 {
        self.frame_id
    }


    /// Display generation, or zero for an error.
    pub fn display_generation(&self) -> u64 {
        self.display_generation
    }


    /// Complete native display width, equal to returned width.
    pub fn width(&self) -> u32 {
        self.width
    }


    /// Complete native display height, equal to returned height.
    pub fn height(&self) -> u32 {
        self.height
    }


    /// Packed BGR8 bytes per row; odd pixel widths need no padding.
    pub fn stride(&self) -> u32 {
        self.stride
    }


    /// Raw decoded and transmitted pixel length; compression is unsupported.
    pub fn pixel_bytes(&self) -> usize {
        self.pixel_bytes
    }


    /// Canonical RGBA bytes per row after channel reordering and alpha insertion.
    pub fn rgba_stride(&self) -> usize {
        self.width as usize * 4
    }


    /// Canonical RGBA allocation length, checked before any pixel allocation.
    pub fn rgba_bytes(&self) -> usize {
        self.rgba_stride() * self.height as usize
    }
}


/// Decode a four-byte field from a previously length-checked fixed-size record.
fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}


/// Decode an eight-byte field from a previously length-checked fixed-size record.
fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
        bytes[offset + 4],
        bytes[offset + 5],
        bytes[offset + 6],
        bytes[offset + 7],
    ])
}
