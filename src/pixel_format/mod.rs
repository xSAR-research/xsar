//! Pixel format conversions for guest capture and image processing.
//!
//! Provides zero-copy, in-place conversion between common 32-bit pixel formats.
//! Scalar implementation auto-vectorises on x64/ARM64; specialised variants
//! (SSE/NEON) are added only if profiling identifies a real bottleneck.

/// Convert BGRA pixels to RGBA in-place.
///
/// Swaps red and blue channels for each 4-byte pixel. Operates on mutable buffer;
/// caller must allocate or pre-size for same byte length. Panics if buffer length
/// is not a multiple of 4.
///
/// # Panics
/// If `pixels.len() % 4 != 0`.
///
/// # Example
/// ```rust
/// use xsar::pixel_format::bgra_to_rgba_inplace;
///
/// let mut bgra = vec![0xAA, 0xBB, 0xCC, 0xDD]; // [B, G, R, A]
/// bgra_to_rgba_inplace(&mut bgra);
/// // Now: [0xCC, 0xBB, 0xAA, 0xDD] => [R, G, B, A]
/// ```
pub fn bgra_to_rgba_inplace(pixels: &mut [u8]) {
    assert!(
        pixels.len().is_multiple_of(4),
        "pixel buffer must be a multiple of 4 bytes"
    );

    let (chunks, _) = pixels.as_chunks_mut::<4>();
    for chunk in chunks {
        // [B, G, R, A] -> [R, G, B, A]
        (chunk[0], chunk[2]) = (chunk[2], chunk[0]);
    }
}

/// Convert BGRA pixels to RGBA, returning owned vec.
///
/// Allocates new buffer and swaps red and blue channels. Returns empty vec
/// if input length is not a multiple of 4.
///
/// # Example
/// ```rust
/// use xsar::pixel_format::bgra_to_rgba;
///
/// let bgra = vec![0xAA, 0xBB, 0xCC, 0xDD];
/// let rgba = bgra_to_rgba(&bgra);
/// // rgba: [0xCC, 0xBB, 0xAA, 0xDD]
/// ```
pub fn bgra_to_rgba(bgra: &[u8]) -> Vec<u8> {
    if !bgra.len().is_multiple_of(4) {
        return Vec::new();
    }

    let mut rgba = Vec::with_capacity(bgra.len());
    let (chunks, _) = bgra.as_chunks::<4>();
    for chunk in chunks {
        // [B, G, R, A] -> [R, G, B, A]
        rgba.extend_from_slice(&[chunk[2], chunk[1], chunk[0], chunk[3]]);
    }
    rgba
}

/// Convert BGRA pixels to RGB (discarding alpha), returning owned vec.
///
/// Drops the alpha byte and outputs 3 bytes per pixel. Returns empty vec
/// if input length is not a multiple of 4.
///
/// # Example
/// ```rust
/// use xsar::pixel_format::bgra_to_rgb;
///
/// let bgra = vec![0xAA, 0xBB, 0xCC, 0xDD];
/// let rgb = bgra_to_rgb(&bgra);
/// // rgb: [0xCC, 0xBB, 0xAA]
/// ```
pub fn bgra_to_rgb(bgra: &[u8]) -> Vec<u8> {
    if !bgra.len().is_multiple_of(4) {
        return Vec::new();
    }

    let mut rgb = Vec::with_capacity(bgra.len() / 4 * 3);
    let (chunks, _) = bgra.as_chunks::<4>();
    for chunk in chunks {
        // [B, G, R, A] -> [R, G, B] (skip alpha)
        rgb.extend_from_slice(&[chunk[2], chunk[1], chunk[0]]);
    }
    rgb
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bgra_to_rgba_inplace() {
        let mut pixels = vec![0xAA, 0xBB, 0xCC, 0xDD, 0x11, 0x22, 0x33, 0x44];
        bgra_to_rgba_inplace(&mut pixels);

        // [B, G, R, A] -> [R, G, B, A]
        assert_eq!(pixels, vec![0xCC, 0xBB, 0xAA, 0xDD, 0x33, 0x22, 0x11, 0x44]);
    }

    #[test]
    fn test_bgra_to_rgba_owned() {
        let bgra = vec![0xAA, 0xBB, 0xCC, 0xDD];
        let rgba = bgra_to_rgba(&bgra);

        assert_eq!(rgba, vec![0xCC, 0xBB, 0xAA, 0xDD]);
    }

    #[test]
    fn test_bgra_to_rgba_multiple_pixels() {
        let bgra = vec![
            0xFF, 0x00, 0x00, 0xFF, // Red pixel (BGRA)
            0x00, 0xFF, 0x00, 0xFF, // Green pixel (BGRA)
            0x00, 0x00, 0xFF, 0xFF, // Blue pixel (BGRA)
        ];
        let rgba = bgra_to_rgba(&bgra);

        assert_eq!(
            rgba,
            vec![
                0x00, 0x00, 0xFF, 0xFF, // Red pixel (RGBA)
                0x00, 0xFF, 0x00, 0xFF, // Green pixel (RGBA)
                0xFF, 0x00, 0x00, 0xFF, // Blue pixel (RGBA)
            ]
        );
    }

    #[test]
    fn test_bgra_to_rgb() {
        let bgra = vec![0xAA, 0xBB, 0xCC, 0xDD];
        let rgb = bgra_to_rgb(&bgra);

        assert_eq!(rgb, vec![0xCC, 0xBB, 0xAA]);
    }

    #[test]
    fn test_bgra_to_rgb_multiple_pixels() {
        let bgra = vec![
            0xFF, 0x00, 0x00, 0xFF, // Red pixel (BGRA)
            0x00, 0xFF, 0x00, 0xFF, // Green pixel (BGRA)
        ];
        let rgb = bgra_to_rgb(&bgra);

        assert_eq!(
            rgb,
            vec![
                0x00, 0x00, 0xFF, // Red pixel (RGB)
                0x00, 0xFF, 0x00, // Green pixel (RGB)
            ]
        );
    }

    #[test]
    fn test_invalid_length_bgra_to_rgba() {
        let bgra = vec![0xAA, 0xBB, 0xCC]; // Not a multiple of 4
        let rgba = bgra_to_rgba(&bgra);

        assert!(rgba.is_empty());
    }

    #[test]
    fn test_invalid_length_bgra_to_rgb() {
        let bgra = vec![0xAA, 0xBB, 0xCC]; // Not a multiple of 4
        let rgb = bgra_to_rgb(&bgra);

        assert!(rgb.is_empty());
    }

    #[test]
    #[should_panic(expected = "pixel buffer must be a multiple of 4 bytes")]
    fn test_bgra_to_rgba_inplace_panic() {
        let mut pixels = vec![0xAA, 0xBB, 0xCC];
        bgra_to_rgba_inplace(&mut pixels);
    }

    #[test]
    fn test_bgra_to_rgba_empty() {
        let bgra = vec![];
        let rgba = bgra_to_rgba(&bgra);

        assert!(rgba.is_empty());
    }

    #[test]
    fn test_bgra_to_rgba_inplace_empty() {
        let mut pixels = vec![];
        bgra_to_rgba_inplace(&mut pixels); // Should not panic

        assert!(pixels.is_empty());
    }

    #[test]
    fn test_bgra_to_rgba_alpha_preserved() {
        // Ensure alpha channel is preserved across conversions
        let bgra = vec![0x12, 0x34, 0x56, 0xAB, 0x78, 0x9A, 0xBC, 0xDE];
        let rgba = bgra_to_rgba(&bgra);

        // Alpha bytes should be at positions 3 and 7
        assert_eq!(rgba[3], 0xAB);
        assert_eq!(rgba[7], 0xDE);
    }
}
