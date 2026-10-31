//! Checked joint RGB changes in little-endian packed pixels, with optional early exit.
//!
//! RGB occupy byte positions zero through two of `u32::to_le_bytes`; position
//! three is ignored. A pixel changes only if every RGB delta exceeds tolerance.
//! Callers own pixel geometry, baseline freshness and action-completion policy.

use std::{error::Error, fmt};


/// Region comparison whose count may cover only a prefix after early refusal.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PackedRgbComparison {
    /// Full equal input length; the percentage denominator even after early exit.
    pub total_pixels: usize,
    /// Number of pixels actually examined, including the complete final batch.
    pub compared_pixels: usize,
    /// Exact changed count in that prefix; a lower bound for the full region.
    pub changed_pixels: usize,
    /// Greatest count permitted by the supplied percentage, rounded down exactly.
    pub allowed_changes: usize,
}


impl PackedRgbComparison {
    /// Whether the measured prefix already strictly exceeds the full-region allowance.
    pub fn exceeds_allowance(&self) -> bool {
        self.changed_pixels > self.allowed_changes
    }
}


/// Invalid input rejected before examining either packed slice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PackedRgbError {
    /// Equal-coordinate comparison requires equal lengths; neither is truncated.
    LengthMismatch {
        /// First input length in pixels.
        before: usize,
        /// Second input length in pixels.
        after: usize,
    },
    /// A whole-region allowance must be within zero through one hundred percent.
    InvalidPercentage(u8),
}


impl fmt::Display for PackedRgbError {
    /// Explain the rejected contract without reading pixel contents.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LengthMismatch { before, after } => {
                write!(
                    formatter,
                    "packed RGB lengths differ: {before} versus {after}"
                )
            }
            Self::InvalidPercentage(percent) => {
                write!(formatter, "packed RGB percentage exceeds 100: {percent}")
            }
        }
    }
}


impl Error for PackedRgbError {}


/// Compute `floor(pixels * percent / 100)` without an overflowing multiplication.
/// The caller validates `percent <= 100`, so each product and the sum fit `usize`.
fn allowance(pixels: usize, percent: u8) -> usize {
    let percent = usize::from(percent);
    (pixels / 100) * percent + ((pixels % 100) * percent) / 100
}


/// Count corresponding pixels for which all three absolute RGB deltas exceed tolerance.
/// Alpha is ignored; byte conversion is independent of the host's endianness.
fn count_changed(before: &[u32], after: &[u32], tolerance: u8) -> usize {
    before
        .iter()
        .zip(after)
        .fold(0, |count, (&before, &after)| {
            let [br, bg, bb, _] = before.to_le_bytes();
            let [ar, ag, ab, _] = after.to_le_bytes();
            count
                + usize::from(br.abs_diff(ar).min(bg.abs_diff(ag)).min(bb.abs_diff(ab)) > tolerance)
        })
}


/// Compare equally sized little-endian RGB-plus-alpha words using a joint RGB predicate.
///
/// `min(abs(R1-R2), abs(G1-G2), abs(B1-B2)) > tolerance` counts one pixel.
/// Differences equal to tolerance, changes in fewer than three channels, and
/// alpha-only changes do not count. The region exceeds its allowance only when
/// `changed_pixels > floor(total_pixels * maximum_changed_percent / 100)`.
///
/// Complete 64-pixel batches are examined before testing early exit. On early
/// exit `changed_pixels` is exact for `compared_pixels`, not necessarily the full
/// region; the decision already uses the full denominator. With a 100-percent
/// allowance the full count is always obtained. Empty equal inputs succeed with
/// zero counts. The safe loop allocates nothing, uses no intrinsics or timers,
/// and makes no SIMD or latency guarantee.
///
/// # Errors
/// Returns [`PackedRgbError::LengthMismatch`] or
/// [`PackedRgbError::InvalidPercentage`] before any pixel is examined.
pub fn compare_packed_rgb(
    before: &[u32],
    after: &[u32],
    tolerance: u8,
    maximum_changed_percent: u8,
) -> Result<PackedRgbComparison, PackedRgbError> {
    if before.len() != after.len() {
        return Err(PackedRgbError::LengthMismatch {
            before: before.len(),
            after: after.len(),
        });
    }
    if maximum_changed_percent > 100 {
        return Err(PackedRgbError::InvalidPercentage(maximum_changed_percent));
    }
    let mut result = PackedRgbComparison {
        total_pixels: before.len(),
        allowed_changes: allowance(before.len(), maximum_changed_percent),
        ..PackedRgbComparison::default()
    };
    let (before_batches, before_tail) = before.as_chunks::<64>();
    let (after_batches, after_tail) = after.as_chunks::<64>();
    for (before, after) in before_batches.iter().zip(after_batches) {
        result.changed_pixels += count_changed(before, after, tolerance);
        result.compared_pixels += before.len();
        if result.exceeds_allowance() {
            return Ok(result);
        }
    }
    result.changed_pixels += count_changed(before_tail, after_tail, tolerance);
    result.compared_pixels += before_tail.len();
    Ok(result)
}


#[cfg(test)]
mod tests {
    use super::*;


    /// Strict per-pixel conjunction cannot be replaced by independent channel totals.
    #[test]
    fn packed_rgb_requires_all_channels_at_the_same_pixel_and_ignores_alpha() {
        let before = [u32::from_le_bytes([100, 100, 100, 0]); 5];
        let after = [
            [127, 127, 127, 255],
            [73, 73, 73, 0],
            [127, 127, 126, 255],
            [255, 100, 100, 255],
            [100, 100, 100, 255],
        ]
        .map(u32::from_le_bytes);
        let measured = compare_packed_rgb(&before, &after, 26, 100).unwrap();
        assert_eq!(measured.changed_pixels, 2);
        assert_eq!(measured.compared_pixels, 5);
        let disjoint =
            [[255, 255, 100, 0], [100, 255, 255, 0], [255, 100, 255, 0]].map(u32::from_le_bytes);
        assert_eq!(
            compare_packed_rgb(&before[..3], &disjoint, 26, 0)
                .unwrap()
                .changed_pixels,
            0
        );
    }


    /// Batches and remainders include boundary and final pixels exactly once.
    #[test]
    fn packed_rgb_batch_and_tail_boundaries() {
        for length in [1, 63, 64, 65, 127, 128, 129] {
            let before = vec![0; length];
            let mut after = before.clone();
            after[length - 1] = u32::from_le_bytes([27, 27, 27, 255]);
            let measured = compare_packed_rgb(&before, &after, 26, 100).unwrap();
            assert_eq!(
                (measured.changed_pixels, measured.compared_pixels),
                (1, length)
            );
        }
    }


    /// Percentage equality is permitted; only the next joint changed pixel exceeds it.
    #[test]
    fn packed_rgb_strict_coverage_and_early_prefix_counts() {
        let before = vec![0; 1519];
        let mut after = before.clone();
        after[..151].fill(u32::from_le_bytes([27, 27, 27, 0]));
        assert!(
            !compare_packed_rgb(&before, &after, 26, 10)
                .unwrap()
                .exceeds_allowance()
        );
        after[151] = u32::from_le_bytes([27, 27, 27, 0]);
        let measured = compare_packed_rgb(&before, &after, 26, 10).unwrap();
        assert!(measured.exceeds_allowance());
        assert_eq!(measured.allowed_changes, 151);
        assert_eq!(measured.changed_pixels, 152);
        assert!(measured.compared_pixels < 1519);
        let mut after = vec![0; 128];
        after[0] = u32::from_le_bytes([27; 4]);
        after[64] = after[0];
        let measured = compare_packed_rgb(&before[..128], &after, 26, 0).unwrap();
        assert_eq!((measured.changed_pixels, measured.compared_pixels), (1, 64));
    }


    /// Validation, empty regions and exact large-integer arithmetic do not inspect storage.
    #[test]
    fn packed_rgb_validation_and_overflow_free_allowance() {
        assert_eq!(
            compare_packed_rgb(&[], &[], 26, 10).unwrap(),
            PackedRgbComparison::default()
        );
        assert!(matches!(
            compare_packed_rgb(&[0], &[], 26, 10),
            Err(PackedRgbError::LengthMismatch { .. })
        ));
        assert_eq!(
            compare_packed_rgb(&[], &[], 26, 101),
            Err(PackedRgbError::InvalidPercentage(101))
        );
        for pixels in [0, 1, 99, 100, 101, usize::MAX / 100 + 1, usize::MAX] {
            assert_eq!(allowance(pixels, 100), pixels);
            assert_eq!(allowance(pixels, 0), 0);
        }
        assert_eq!(allowance(99, 1), 0);
        assert_eq!(allowance(101, 1), 1);
        let b = [0];
        let a = [u32::from_le_bytes([255, 255, 255, 0])];
        assert_eq!(
            compare_packed_rgb(&b, &a, 255, 0).unwrap().changed_pixels,
            0
        );
    }
}
