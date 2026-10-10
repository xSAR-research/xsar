//! Exact, read-only RGB change counts between independently validated frames.
//!
//! Coordinates are full-frame pixels. Regions and exclusions are non-empty,
//! checked half-open rectangles wholly inside the frame. Exclusions contribute
//! only their intersection with the region and overlapping exclusions form a
//! union. Callers retain thresholds, calibration and all effect/input policy.

use std::fmt;

use super::{FrameView, MatchError};
use crate::geometry::PixelRect;


/// Invalid comparison geometry or incompatible validated frame dimensions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComparisonError {
    /// Both frames are valid independently but their dimensions differ.
    DimensionsDiffer {
        /// Width and height of the earlier frame.
        before: (u32, u32),
        /// Width and height of the later frame.
        after: (u32, u32),
    },
    /// The region or a checked pixel read is invalid.
    InvalidInput(MatchError),
    /// One exclusion is empty, overflowing or outside the frame.
    InvalidExclusion {
        /// Zero-based index into the caller's exclusion slice.
        index: usize,
        /// The underlying rectangle validation failure.
        source: MatchError,
    },
}


impl fmt::Display for ComparisonError {
    /// Describe the rejected comparison without assigning application policy.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DimensionsDiffer { before, after } => write!(
                formatter,
                "comparison dimensions differ: {}x{} versus {}x{}",
                before.0, before.1, after.0, after.1
            ),
            Self::InvalidInput(source) => write!(formatter, "invalid comparison input: {source}"),
            Self::InvalidExclusion { index, source } => {
                write!(formatter, "invalid comparison exclusion {index}: {source}")
            }
        }
    }
}


impl std::error::Error for ComparisonError {
    /// Retain the underlying checked-image error when one exists.
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::DimensionsDiffer { .. } => None,
            Self::InvalidInput(source) | Self::InvalidExclusion { source, .. } => Some(source),
        }
    }
}


/// Count unexcluded pixels with any absolute RGB channel delta >= `threshold`.
///
/// Construct each view with [`FrameView::from_rgba`] or
/// [`super::CapturedFrame::as_view`] to validate stride and storage independently.
/// Dimensions must agree; strides may differ. Alpha and row padding are ignored.
/// Threshold zero counts every unexcluded pixel, even if both frames are equal;
/// threshold 255 requires a full-range difference in at least one RGB channel.
///
/// All exclusions are validated before scanning, even if another exclusion
/// already covers the entire region. Valid exclusions may be disjoint from the
/// region. Duplicates and overlaps exclude each pixel once. Complete exclusion
/// and valid zero change both return `Ok(0)`; invalid input always returns an error.
///
/// No allocation, I/O, unsafe code or early effect-threshold exit is performed.
/// For region area `A` and exclusion count `E`, worst-case time is
/// `O(E + A * (E + 1))`, with constant auxiliary storage. There is no fixed mask
/// count cap. The exact count is bounded by `u64::from(width) * u64::from(height)`;
/// two `u32` dimensions cannot overflow `u64`. Storage remains bounded by the
/// independently validated borrowed slices and their checked `usize` layouts.
///
/// # Errors
///
/// Returns [`ComparisonError::DimensionsDiffer`] for unequal frame dimensions,
/// [`ComparisonError::InvalidInput`] for invalid region geometry or pixel access,
/// and [`ComparisonError::InvalidExclusion`] for the first invalid exclusion.
///
/// # Examples
///
/// ```
/// use xsar::geometry::PixelRect;
/// use xsar::image_matching::{FrameView, comparison::count_rgb_changes};
///
/// let before_bytes = [0, 0, 0, 255, 0, 0, 0, 255];
/// let after_bytes = [20, 0, 0, 0, 0, 0, 19, 255];
/// let before = FrameView::from_rgba(2, 1, 8, &before_bytes)?;
/// let after = FrameView::from_rgba(2, 1, 8, &after_bytes)?;
/// let changed = count_rgb_changes(before, after, PixelRect::new(0, 0, 2, 1), &[], 20)?;
/// assert_eq!(changed, 1);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn count_rgb_changes(
    before: FrameView<'_>,
    after: FrameView<'_>,
    bounds: PixelRect,
    exclusions: &[PixelRect],
    threshold: u8,
) -> Result<u64, ComparisonError> {
    let before_dimensions = (before.width(), before.height());
    let after_dimensions = (after.width(), after.height());


    if before_dimensions != after_dimensions {
        return Err(ComparisonError::DimensionsDiffer {
            before: before_dimensions,
            after: after_dimensions,
        });
    }
    let (right, bottom) = before
        .checked_bounds(bounds)
        .map_err(ComparisonError::InvalidInput)?;


    for (index, exclusion) in exclusions.iter().enumerate() {
        before
            .checked_bounds(*exclusion)
            .map_err(|source| ComparisonError::InvalidExclusion { index, source })?;
    }
    let mut changed = 0_u64;


    for row in bounds.y..bottom {
        for column in bounds.x..right {
            if exclusions.iter().any(|exclusion| {
                column >= exclusion.x
                    && column < exclusion.right()
                    && row >= exclusion.y
                    && row < exclusion.bottom()
            }) {
                continue;
            }
            let old = before
                .pixel_rgb(column, row)
                .ok_or(ComparisonError::InvalidInput(
                    MatchError::InvalidFrameLayout,
                ))?;
            let new = after
                .pixel_rgb(column, row)
                .ok_or(ComparisonError::InvalidInput(
                    MatchError::InvalidFrameLayout,
                ))?;


            if old
                .into_iter()
                .zip(new)
                .any(|(old, new)| old.abs_diff(new) >= threshold)
            {
                changed += 1;
            }
        }
    }
    Ok(changed)
}


#[cfg(test)]
mod tests {
    //! Synthetic boundary and exclusion regressions independent of any game.

    use super::*;


    /// Compare packed single-row RGB evidence with opaque alpha bytes.
    fn compare_row(before: &[u8], after: &[u8], threshold: u8) -> u64 {
        let width = u32::try_from(before.len() / 4).unwrap();
        count_rgb_changes(
            FrameView::from_rgba(width, 1, before.len(), before).unwrap(),
            FrameView::from_rgba(width, 1, after.len(), after).unwrap(),
            PixelRect::new(0, 0, width, 1),
            &[],
            threshold,
        )
        .unwrap()
    }


    /// Every RGB channel independently qualifies at the inclusive threshold.
    #[test]
    fn channel_thresholds_are_inclusive_and_symmetric() {
        let before = [100_u8; 20];
        let after = [
            119, 100, 100, 100, 120, 100, 100, 100, 100, 80, 100, 100, 100, 100, 121, 100, 120, 80,
            121, 100,
        ];
        assert_eq!(compare_row(&before, &after, 20), 4);
        assert_eq!(compare_row(&after, &before, 20), 4);
        assert_eq!(compare_row(&before, &after, 21), 2);
        assert_eq!(compare_row(&before, &after, 22), 0);
    }


    /// Threshold zero includes equality; 255 requires an extreme RGB difference.
    #[test]
    fn zero_and_maximum_thresholds_ignore_alpha() {
        let before = [0, 0, 0, 0, 255, 255, 255, 255, 0, 0, 0, 0];
        let after = [255, 0, 0, 0, 255, 0, 255, 255, 0, 0, 254, 255];
        assert_eq!(compare_row(&before, &after, 255), 2);
        assert_eq!(compare_row(&before, &after, 254), 3);
        assert_eq!(compare_row(&before, &before, 0), 3);
        assert_eq!(compare_row(&[1, 2, 3, 0], &[1, 2, 3, 255], 1), 0);
        assert_eq!(compare_row(&[1, 2, 3, 0], &[1, 2, 3, 255], 0), 1);
    }


    /// Different valid row strides, padding and trailing bytes do not alter counts.
    #[test]
    fn independent_padded_strides_preserve_rgb_measurements() {
        let before = [0_u8; 19];
        let mut after = [255_u8; 27];


        for row in 0..2 {
            after[row * 12..row * 12 + 4].copy_from_slice(&[0, 0, 0, 255]);
        }
        let old = FrameView::from_rgba(1, 2, 8, &before).unwrap();
        let bounds = PixelRect::new(0, 0, 1, 2);
        assert_eq!(
            count_rgb_changes(
                old,
                FrameView::from_rgba(1, 2, 12, &after).unwrap(),
                bounds,
                &[],
                20
            ),
            Ok(0)
        );
        after[12 + 2] = 20;
        let new = FrameView::from_rgba(1, 2, 12, &after).unwrap();
        assert_eq!(count_rgb_changes(old, new, bounds, &[], 20), Ok(1));
        assert_eq!(count_rgb_changes(new, old, bounds, &[], 20), Ok(1));
    }


    /// The first and last included pixels count, while all four outer edges do not.
    #[test]
    fn region_edges_are_half_open() {
        let before = [0_u8; 64];
        let mut after = [0_u8; 64];


        for (column, row) in [(1, 1), (2, 2), (0, 1), (3, 1), (1, 0), (1, 3)] {
            after[row * 16 + column * 4] = 255;
        }
        let old = FrameView::from_rgba(4, 4, 16, &before).unwrap();
        let new = FrameView::from_rgba(4, 4, 16, &after).unwrap();
        assert_eq!(
            count_rgb_changes(old, new, PixelRect::new(1, 1, 2, 2), &[], 255),
            Ok(2)
        );
        assert_eq!(
            count_rgb_changes(old, new, PixelRect::new(0, 0, 4, 4), &[], 255),
            Ok(6)
        );
        assert_eq!(
            count_rgb_changes(old, new, PixelRect::new(0, 0, 4, 4), &[], 0),
            Ok(16)
        );
    }


    /// Three masks intersect the ROI as a union regardless of duplicates or order.
    #[test]
    fn exclusions_use_intersection_and_union() {
        let before = [0_u8; 160];
        let after = [255_u8; 160];
        let old = FrameView::from_rgba(8, 5, 32, &before).unwrap();
        let new = FrameView::from_rgba(8, 5, 32, &after).unwrap();
        let bounds = PixelRect::new(1, 1, 6, 3);
        let exclusions = [
            PixelRect::new(0, 0, 3, 3),
            PixelRect::new(2, 2, 3, 3),
            PixelRect::new(7, 0, 1, 5),
        ];
        assert_eq!(count_rgb_changes(old, new, bounds, &[], 20), Ok(18));
        assert_eq!(
            count_rgb_changes(old, new, bounds, &exclusions[..1], 20),
            Ok(14)
        );
        assert_eq!(count_rgb_changes(old, new, bounds, &exclusions, 20), Ok(9));
        let reordered = [exclusions[2], exclusions[1], exclusions[0], exclusions[1]];
        assert_eq!(count_rgb_changes(new, old, bounds, &reordered, 20), Ok(9));
        assert_eq!(count_rgb_changes(old, old, bounds, &exclusions, 0), Ok(9));
        assert_eq!(count_rgb_changes(old, new, bounds, &[bounds], 0), Ok(0));
        assert_eq!(count_rgb_changes(old, old, bounds, &[], 20), Ok(0));
    }


    /// Invalid ROI geometry is an error even when a valid exclusion covers it.
    #[test]
    fn invalid_regions_refuse_instead_of_returning_zero() {
        let bytes = [0_u8; 64];
        let frame = FrameView::from_rgba(4, 4, 16, &bytes).unwrap();


        for bounds in [PixelRect::new(0, 0, 0, 1), PixelRect::new(0, 0, 1, 0)] {
            assert_eq!(
                count_rgb_changes(frame, frame, bounds, &[], 20),
                Err(ComparisonError::InvalidInput(MatchError::EmptyBounds))
            );
        }


        for bounds in [
            PixelRect::new(3, 0, 2, 1),
            PixelRect::new(0, 3, 1, 2),
            PixelRect::new(u32::MAX, 0, 2, 1),
            PixelRect::new(0, u32::MAX, 1, 2),
        ] {
            assert_eq!(
                count_rgb_changes(frame, frame, bounds, &[PixelRect::new(0, 0, 4, 4)], 20),
                Err(ComparisonError::InvalidInput(
                    MatchError::BoundsOutsideFrame
                ))
            );
        }
    }


    /// Every mask is validated before pixels, including masks after full coverage.
    #[test]
    fn invalid_exclusions_identify_the_first_bad_index() {
        let bytes = [0_u8; 64];
        let frame = FrameView::from_rgba(4, 4, 16, &bytes).unwrap();
        let bounds = PixelRect::new(0, 0, 4, 4);


        for (invalid, source) in [
            (PixelRect::new(0, 0, 0, 1), MatchError::EmptyBounds),
            (PixelRect::new(0, 0, 1, 0), MatchError::EmptyBounds),
            (PixelRect::new(3, 0, 2, 1), MatchError::BoundsOutsideFrame),
            (PixelRect::new(0, 3, 1, 2), MatchError::BoundsOutsideFrame),
            (
                PixelRect::new(u32::MAX, 0, 2, 1),
                MatchError::BoundsOutsideFrame,
            ),
            (
                PixelRect::new(0, u32::MAX, 1, 2),
                MatchError::BoundsOutsideFrame,
            ),
        ] {
            assert_eq!(
                count_rgb_changes(frame, frame, bounds, &[bounds, invalid], 20),
                Err(ComparisonError::InvalidExclusion { index: 1, source })
            );
        }
    }


    /// Dimensions must match even if the requested region fits both frames.
    #[test]
    fn independent_valid_frames_still_require_equal_dimensions() {
        let bytes = [0_u8; 64];
        let before = FrameView::from_rgba(4, 4, 16, &bytes).unwrap();


        for dimensions in [(3, 4), (4, 3)] {
            let after = FrameView::from_rgba(dimensions.0, dimensions.1, 16, &bytes).unwrap();
            assert_eq!(
                count_rgb_changes(before, after, PixelRect::new(0, 0, 1, 1), &[], 20),
                Err(ComparisonError::DimensionsDiffer {
                    before: (4, 4),
                    after: dimensions
                })
            );
        }
    }


    /// Malformed layouts cannot construct either comparison view.
    #[test]
    fn malformed_storage_and_arithmetic_refuse_at_view_construction() {
        for (width, height, stride, length) in [
            (2, 2, 7, 16),
            (2, 2, 8, 15),
            (2, 2, 12, 23),
            (1, 2, usize::MAX, 0),
            (u32::MAX, u32::MAX, usize::MAX, 0),
        ] {
            let bytes = vec![0_u8; length];
            assert_eq!(
                FrameView::from_rgba(width, height, stride, &bytes).unwrap_err(),
                MatchError::InvalidFrameLayout
            );
        }
        let empty = FrameView::from_rgba(0, 0, 0, &[]).unwrap();
        assert_eq!(
            count_rgb_changes(empty, empty, PixelRect::new(0, 0, 1, 1), &[], 20),
            Err(ComparisonError::InvalidInput(
                MatchError::BoundsOutsideFrame
            ))
        );
    }


    /// Diagnostics retain geometry identity and the original image error source.
    #[test]
    fn errors_preserve_context_and_sources() {
        use std::error::Error;

        let dimensions = ComparisonError::DimensionsDiffer {
            before: (1, 2),
            after: (3, 4),
        };
        assert_eq!(
            dimensions.to_string(),
            "comparison dimensions differ: 1x2 versus 3x4"
        );
        assert!(dimensions.source().is_none());
        let exclusion = ComparisonError::InvalidExclusion {
            index: 2,
            source: MatchError::EmptyBounds,
        };
        assert!(exclusion.to_string().contains("exclusion 2"));
        assert_eq!(
            exclusion.source().unwrap().to_string(),
            MatchError::EmptyBounds.to_string()
        );
        assert!(
            ComparisonError::InvalidInput(MatchError::BoundsOutsideFrame)
                .source()
                .is_some()
        );
    }
}
