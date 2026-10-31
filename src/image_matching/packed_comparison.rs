//! Independent absolute byte differences in two equally sized packed arrays.
//!
//! Byte positions use `u32::to_le_bytes`: position zero is the least significant
//! byte, regardless of host endianness. Callers own the meaning of those positions,
//! acceptable coverage/magnitude and any temporal or action-completion policy.
//! The safe single-pass loop allocates nothing and performs no timing or logging.

use std::{error::Error, fmt};


/// Exact integer measurements for all four independent byte positions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PackedByteComparison {
    /// Number of compared `u32` words, also the denominator for each byte position.
    pub words: u64,
    /// Counts of absolute differences strictly greater than the supplied tolerance.
    pub changed: [u64; 4],
    /// Sums of all absolute differences, including differences within tolerance.
    pub absolute_difference: [u64; 4],
    /// Largest absolute difference at each byte position, or zero for empty arrays.
    pub maximum_difference: [u8; 4],
}


impl PackedByteComparison {
    /// Return changed bytes as a percentage of all four positions in all words.
    ///
    /// Empty comparisons return zero. Use the integer fields for exact threshold
    /// decisions or to exclude a byte position such as alpha.
    pub fn changed_percentage(&self) -> f64 {
        if self.words == 0 {
            return 0.0;
        }
        100.0 * self.changed.iter().map(|&count| count as f64).sum::<f64>()
            / (self.words as f64 * 4.0)
    }
}


/// A comparison rejected before reading either array or producing partial results.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PackedComparisonError {
    /// Arrays have different numbers of words; neither is truncated.
    LengthMismatch {
        /// Number of words in the first array.
        before: usize,
        /// Number of words in the second array.
        after: usize,
    },
    /// Worst-case four-byte difference totals cannot be represented as `u64`.
    InputTooLong,
}


impl fmt::Display for PackedComparisonError {
    /// Describe the rejected length contract without inspecting array contents.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LengthMismatch { before, after } => {
                write!(
                    formatter,
                    "packed comparison lengths differ: {before} versus {after}"
                )
            }
            Self::InputTooLong => formatter.write_str("packed comparison totals would overflow"),
        }
    }
}


impl Error for PackedComparisonError {}


/// Measure independent absolute byte differences in equally sized `u32` slices.
///
/// A byte is changed only when `abs(before - after) > tolerance`; equality to the
/// tolerance is unchanged. Every byte contributes its absolute difference to the
/// magnitude, so lighter and darker changes never cancel and packed-word borrow
/// cannot cross byte boundaries. All four positions, including possible alpha,
/// are measured separately. Empty arrays succeed with zero measurements.
///
/// # Errors
/// Returns [`PackedComparisonError::LengthMismatch`] for unequal lengths, or
/// [`PackedComparisonError::InputTooLong`] if worst-case totals could overflow.
/// No allocation, unchecked access, architecture intrinsics or timer is used.
pub fn compare_packed_bytes(
    before: &[u32],
    after: &[u32],
    tolerance: u8,
) -> Result<PackedByteComparison, PackedComparisonError> {
    if before.len() != after.len() {
        return Err(PackedComparisonError::LengthMismatch {
            before: before.len(),
            after: after.len(),
        });
    }
    let words = u64::try_from(before.len()).map_err(|_| PackedComparisonError::InputTooLong)?;
    if words > u64::MAX / (4 * u64::from(u8::MAX)) {
        return Err(PackedComparisonError::InputTooLong);
    }
    let mut result = PackedByteComparison {
        words,
        ..PackedByteComparison::default()
    };
    for (&before, &after) in before.iter().zip(after) {
        let before = before.to_le_bytes();
        let after = after.to_le_bytes();
        for channel in 0..4 {
            let difference = before[channel].abs_diff(after[channel]);
            result.changed[channel] += u64::from(difference > tolerance);
            result.absolute_difference[channel] += u64::from(difference);
            result.maximum_difference[channel] = result.maximum_difference[channel].max(difference);
        }
    }
    Ok(result)
}


#[cfg(test)]
mod tests {
    use super::*;


    /// Opposite brightness changes accumulate independently without cross-byte borrow.
    #[test]
    fn packed_bytes_measure_both_directions_and_every_position() {
        let before = [
            u32::from_le_bytes([0, 255, 80, 9]),
            u32::from_le_bytes([255, 0, 90, 9]),
        ];
        let after = [
            u32::from_le_bytes([255, 0, 90, 10]),
            u32::from_le_bytes([0, 255, 80, 8]),
        ];
        let measured = compare_packed_bytes(&before, &after, 1).unwrap();
        assert_eq!(measured.words, 2);
        assert_eq!(measured.changed, [2, 2, 2, 0]);
        assert_eq!(measured.absolute_difference, [510, 510, 20, 2]);
        assert_eq!(measured.maximum_difference, [255, 255, 10, 1]);
        assert_eq!(measured.changed_percentage(), 75.0);
    }


    /// Equal, empty and boundary-tolerance inputs obey the exact counting contract.
    #[test]
    fn packed_bytes_empty_lengths_and_tolerance_boundaries() {
        assert_eq!(
            compare_packed_bytes(&[], &[], 0).unwrap(),
            PackedByteComparison::default()
        );
        assert_eq!(
            compare_packed_bytes(&[], &[], 255)
                .unwrap()
                .changed_percentage(),
            0.0
        );
        assert_eq!(compare_packed_bytes(&[0], &[0], 0).unwrap().changed, [0; 4]);
        assert_eq!(
            compare_packed_bytes(&[0], &[0x0403_0201], 2)
                .unwrap()
                .changed,
            [0, 0, 1, 1]
        );
        assert_eq!(
            compare_packed_bytes(&[0], &[u32::MAX], 255)
                .unwrap()
                .changed,
            [0; 4]
        );
        assert_eq!(
            compare_packed_bytes(&[0], &[], 0),
            Err(PackedComparisonError::LengthMismatch {
                before: 1,
                after: 0
            })
        );
    }


    /// Exhaustive single-byte values exercise every delta and byte-position boundary.
    #[test]
    fn packed_bytes_exhaustive_values_do_not_mix_positions() {
        for before in 0..=u8::MAX {
            for after in 0..=u8::MAX {
                let delta = before.abs_diff(after);
                let measured = compare_packed_bytes(
                    &[u32::from_le_bytes([before; 4])],
                    &[u32::from_le_bytes([after; 4])],
                    12,
                )
                .unwrap();
                assert_eq!(measured.changed, [u64::from(delta > 12); 4]);
                assert_eq!(measured.absolute_difference, [u64::from(delta); 4]);
                assert_eq!(measured.maximum_difference, [delta; 4]);
            }
        }
    }
}
