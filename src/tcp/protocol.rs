//! Shared screen-capture TCP protocol, version 2.
//!
//! A capture request is [`REQUEST_BYTES`]. Its response starts with a packed,
//! little-endian [`HEADER_LEN`]-byte header: width (`u32`), height (`u32`),
//! stride (`u16`), and frame number (`u32`), followed by tightly packed,
//! top-down BGRA8 pixels. Each wire pixel is four bytes in B, G, R, A order;
//! the Windows server normalizes alpha to 255. The client converts these to
//! R, G, B, A order, which still uses four bytes per pixel.
//!
//! Stride is width * 4 bytes: 1920 pixels * 4 = 7680 bytes per row. Although
//! `u16::MAX` is 65535, a packed four-byte pixel needs a multiple of four,
//! so the maximum supported stride is 65532 bytes (16383 pixels wide).
//! Convert signed/API values with [`checked_stride`] before narrowing; a
//! negative value already cast to an unsigned wire field cannot be recovered.
//! Buffer lengths and indexing use checked `usize` arithmetic internally.


/// Version-3 BGR8 records; version-2 exports below remain unchanged.
pub mod v3;


/// Version of the legacy screen-capture protocol.
pub const PROTOCOL_VERSION: u8 = 2;


/// Capture-full request: marker, version, command, and two reserved zero bytes.
pub const REQUEST_BYTES: [u8; 5] = [0x42, PROTOCOL_VERSION, 0x01, 0, 0];


/// Packed response-header length; do not serialize a Rust struct's memory.
pub const HEADER_LEN: usize = 14;


/// Existing client limit for a complete uncompressed capture.
const MAX_PIXEL_BYTES: usize = 8 * 1024 * 1024;


/// Validate a signed/API stride before conversion to the unsigned wire field.
///
/// A valid stride is positive, fits `u16`, and contains complete BGRA8 pixels.
pub fn checked_stride(value: i64) -> Result<u16, &'static str> {
    if value <= 0 {
        return Err("stride must be positive");
    }
    let stride = u16::try_from(value).map_err(|_| "stride exceeds 65535 bytes")?;
    if !stride.is_multiple_of(4) {
        return Err("stride must be a multiple of four bytes per BGRA8 pixel");
    }
    Ok(stride)
}


/// Calculate packed BGRA8 row length without truncation or overflow.
///
/// `width * 4` must fit a positive `u16`: at most 16383 pixels / 65532 bytes.
pub fn stride_from_width(width: u32) -> Result<u16, &'static str> {
    let bytes = width
        .checked_mul(4)
        .ok_or("width times four bytes overflows u32")?;
    checked_stride(i64::from(bytes))
}


/// Validated version-2 response geometry and frame number.
///
/// Fields remain private so a parsed or constructed header always describes a
/// positive, tightly packed frame within the 8 MiB pixel-buffer limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameHeader {
    width: u32,
    height: u32,
    stride: u16,
    frame_number: u32,
    pixel_bytes: usize,
}


impl FrameHeader {
    /// Validate packed BGRA geometry and retain its checked buffer length.
    pub fn new(
        width: u32,
        height: u32,
        stride: u16,
        frame_number: u32,
    ) -> Result<Self, &'static str> {
        let expected_stride = stride_from_width(width)?;
        if height == 0 {
            return Err("frame height must be positive");
        }
        if stride != expected_stride {
            return Err("stride must equal width times four bytes per BGRA8 pixel");
        }
        let height_usize =
            usize::try_from(height).map_err(|_| "frame height exceeds host address space")?;
        let pixel_bytes = usize::from(stride)
            .checked_mul(height_usize)
            .ok_or("pixel buffer size overflows host address space")?;
        if pixel_bytes > MAX_PIXEL_BYTES {
            return Err("pixel buffer exceeds 8 MiB limit");
        }
        Ok(Self {
            width,
            height,
            stride,
            frame_number,
            pixel_bytes,
        })
    }


    /// Decode the packed little-endian header and validate before allocation.
    pub fn from_bytes(bytes: [u8; HEADER_LEN]) -> Result<Self, &'static str> {
        Self::new(
            u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
            u16::from_le_bytes([bytes[8], bytes[9]]),
            u32::from_le_bytes([bytes[10], bytes[11], bytes[12], bytes[13]]),
        )
    }


    /// Encode explicitly; the wire header has no alignment padding.
    pub fn to_bytes(&self) -> [u8; HEADER_LEN] {
        let mut bytes = [0; HEADER_LEN];
        bytes[0..4].copy_from_slice(&self.width.to_le_bytes());
        bytes[4..8].copy_from_slice(&self.height.to_le_bytes());
        bytes[8..10].copy_from_slice(&self.stride.to_le_bytes());
        bytes[10..14].copy_from_slice(&self.frame_number.to_le_bytes());
        bytes
    }


    /// Frame width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }


    /// Frame height in pixels; rows are stored top-down.
    pub fn height(&self) -> u32 {
        self.height
    }


    /// Positive bytes per row, represented by two bytes on the wire.
    pub fn stride(&self) -> u16 {
        self.stride
    }


    /// Server sequence number associated with this frame.
    pub fn frame_number(&self) -> u32 {
        self.frame_number
    }


    /// Checked pixel-buffer length, excluding the response header.
    pub fn pixel_bytes(&self) -> usize {
        self.pixel_bytes
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signed_stride_rejects_negative_zero_and_over_limit_before_narrowing() {
        for stride in [i64::MIN, -65536, -4, -1, 0, 65536, 65540, i64::MAX] {
            assert!(checked_stride(stride).is_err(), "accepted {stride}");
        }
        assert_eq!(checked_stride(4), Ok(4));
        assert_eq!(checked_stride(7680), Ok(7680));
        assert_eq!(checked_stride(65532), Ok(65532));
    }
    #[test]
    fn stride_requires_complete_four_byte_pixels() {
        for stride in [1, 2, 3, 5, 65533, 65534, 65535] {
            assert!(checked_stride(stride).is_err(), "accepted {stride}");
        }
    }
    #[test]
    fn packed_stride_respects_width_and_overflow_limits() {
        assert_eq!(stride_from_width(1), Ok(4));
        assert_eq!(stride_from_width(1920), Ok(7680));
        assert_eq!(stride_from_width(16383), Ok(65532));
        for width in [0, 16384, 1 << 30, u32::MAX] {
            assert!(stride_from_width(width).is_err(), "accepted {width}");
        }
    }
    #[test]
    fn request_and_header_have_exact_version_two_wire_layout() {
        assert_eq!(REQUEST_BYTES, [0x42, 0x02, 0x01, 0, 0]);
        let header = FrameHeader::new(2, 3, 8, 0x12345678).unwrap();
        let bytes = [2, 0, 0, 0, 3, 0, 0, 0, 8, 0, 0x78, 0x56, 0x34, 0x12];
        assert_eq!(HEADER_LEN, 14);
        assert_eq!(header.to_bytes(), bytes);
        assert_eq!(FrameHeader::from_bytes(bytes).unwrap(), header);
        assert_eq!(header.width(), 2);
        assert_eq!(header.height(), 3);
        assert_eq!(header.stride(), 8);
        assert_eq!(header.frame_number(), 0x12345678);
        assert_eq!(header.pixel_bytes(), 24);
    }
    #[test]
    fn malformed_geometry_is_rejected_on_construction_and_decode() {
        for (width, height, stride) in [
            (0, 1, 0),
            (1, 0, 4),
            (2, 1, 0),
            (2, 1, 4),
            (2, 1, 9),
            (2, 1, 12),
            (16384, 1, 0),
            (16383, 1, u16::MAX),
        ] {
            assert!(FrameHeader::new(width, height, stride, 0).is_err());
            let mut bytes = [0; HEADER_LEN];
            bytes[0..4].copy_from_slice(&width.to_le_bytes());
            bytes[4..8].copy_from_slice(&height.to_le_bytes());
            bytes[8..10].copy_from_slice(&stride.to_le_bytes());
            assert!(FrameHeader::from_bytes(bytes).is_err());
        }
    }
    #[test]
    fn frame_byte_limit_is_inclusive_and_large_heights_are_rejected() {
        assert_eq!(
            FrameHeader::new(1024, 2048, 4096, 0).unwrap().pixel_bytes(),
            MAX_PIXEL_BYTES
        );
        assert!(FrameHeader::new(1024, 2049, 4096, 0).is_err());
        assert!(FrameHeader::new(16383, u32::MAX, 65532, 0).is_err());
        assert_eq!(
            FrameHeader::new(1920, 1080, 7680, 0).unwrap().pixel_bytes(),
            8_294_400
        );
        assert_eq!(
            FrameHeader::new(16383, 1, 65532, 0).unwrap().pixel_bytes(),
            65532
        );
    }
}
