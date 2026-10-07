//! Checked RGBA frames and deterministic read-only predicate scans.
//!
//! Coordinates are full-frame pixels and rectangles are half-open. Scans report
//! measured evidence; callers retain colour calibration, anchor normalisation,
//! action selection and completion policy. RGB predicates deliberately ignore
//! alpha. These primitives do not perform reference-image template matching.

use std::fmt;

use crate::geometry::{PixelPoint, PixelRect};


/// Supported four-byte, eight-bit-per-channel pixel ordering.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelFormat {
    /// Red, green, blue and alpha in that byte order.
    Rgba8,
}


/// Owned RGBA image storage; consumers validate its public layout before scanning.
///
/// Public fields preserve adapters that construct frames directly. Prefer
/// [`CapturedFrame::as_view`] for checked immutable access to untrusted storage.
#[derive(Clone, Debug)]
pub struct CapturedFrame {
    /// Image width in pixels.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
    /// Number of bytes between successive image rows, including any padding.
    pub stride: usize,
    /// Byte ordering of the owned pixel buffer.
    pub format: PixelFormat,
    /// Top-to-bottom row-major image bytes, including row padding.
    pub pixels: Vec<u8>,
}


impl CapturedFrame {
    /// Return packed RGBA stride, or `usize::MAX` if it is not representable.
    pub fn minimum_stride(&self) -> usize {
        usize::try_from(self.width).ok()
            .and_then(|width| width.checked_mul(4))
            .unwrap_or(usize::MAX)
    }


    /// Report whether checked stride arithmetic and storage cover every row.
    pub fn is_layout_valid(&self) -> bool {
        self.as_view().is_ok()
    }


    /// Validate this frame and borrow immutable pixel storage for repeated scans.
    pub fn as_view(&self) -> Result<FrameView<'_>, MatchError> {
        FrameView::try_from(self)
    }
}


/// Immutable RGBA view whose row layout and total storage were checked once.
#[derive(Clone, Copy, Debug)]
pub struct FrameView<'a> {
    /// Validated frame width.
    width: u32,
    /// Validated frame height.
    height: u32,
    /// Validated byte distance between rows.
    stride: usize,
    /// Borrowed full-row storage.
    pixels: &'a [u8],
}


impl<'a> FrameView<'a> {
    /// Validate borrowed RGBA bytes, including any declared row padding.
    pub fn from_rgba(
        width: u32,
        height: u32,
        stride: usize,
        pixels: &'a [u8],
    ) -> Result<Self, MatchError> {
        let packed_stride = usize::try_from(width).ok()
            .and_then(|width| width.checked_mul(4))
            .ok_or(MatchError::InvalidFrameLayout)?;
        let required = usize::try_from(height).ok()
            .and_then(|height| stride.checked_mul(height))
            .ok_or(MatchError::InvalidFrameLayout)?;

        if stride < packed_stride || pixels.len() < required {
            return Err(MatchError::InvalidFrameLayout);
        }

        Ok(Self { width, height, stride, pixels })
    }


    /// Return the validated width in pixels.
    pub const fn width(self) -> u32 {
        self.width
    }


    /// Return the validated height in pixels.
    pub const fn height(self) -> u32 {
        self.height
    }


    /// Read RGB channels at an in-frame coordinate with checked byte arithmetic.
    ///
    /// Alpha remains in the backing storage but is not passed to RGB predicates.
    pub fn pixel_rgb(self, x: u32, y: u32) -> Option<[u8; 3]> {
        if x >= self.width || y >= self.height {
            return None;
        }

        let row = usize::try_from(y).ok()?.checked_mul(self.stride)?;
        let column = usize::try_from(x).ok()?.checked_mul(4)?;
        let offset = row.checked_add(column)?;
        let end = offset.checked_add(4)?;
        let pixel = self.pixels.get(offset..end)?;
        Some([pixel[0], pixel[1], pixel[2]])
    }


    /// Validate a non-empty half-open ROI and return its exclusive edges.
    pub fn checked_bounds(self, bounds: PixelRect) -> Result<(u32, u32), MatchError> {
        if bounds.is_empty() {
            return Err(MatchError::EmptyBounds);
        }

        let right = bounds.x.checked_add(bounds.width).ok_or(MatchError::BoundsOutsideFrame)?;
        let bottom = bounds.y.checked_add(bounds.height).ok_or(MatchError::BoundsOutsideFrame)?;

        if right > self.width || bottom > self.height {
            return Err(MatchError::BoundsOutsideFrame);
        }

        Ok((right, bottom))
    }
}


impl<'a> TryFrom<&'a CapturedFrame> for FrameView<'a> {
    type Error = MatchError;


    /// Validate public frame metadata before borrowing its pixel bytes.
    fn try_from(frame: &'a CapturedFrame) -> Result<Self, Self::Error> {
        match frame.format {
            PixelFormat::Rgba8 => Self::from_rgba(frame.width, frame.height, frame.stride, &frame.pixels),
        }
    }
}


/// Invalid storage, ROI, coordinate representation or scan configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchError {
    /// Stride, host arithmetic or byte storage cannot cover the declared frame.
    InvalidFrameLayout,
    /// A scan rectangle contains no pixels.
    EmptyBounds,
    /// A half-open ROI overflows or extends beyond the frame.
    BoundsOutsideFrame,
    /// A returned anchor cannot fit the signed [`PixelPoint`] representation.
    AnchorOutsideCoordinateRange,
    /// A run or block minimum must contain at least one pixel.
    ZeroMinimum,
}


impl fmt::Display for MatchError {
    /// Describe the rejected scan precondition without changing application policy.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidFrameLayout => "captured frame has an invalid pixel layout",
            Self::EmptyBounds => "scan bounds must be non-empty",
            Self::BoundsOutsideFrame => "scan bounds lie outside the captured frame",
            Self::AnchorOutsideCoordinateRange => "scan anchor cannot be represented as a signed pixel point",
            Self::ZeroMinimum => "scan minimum must be non-zero",
        };
        formatter.write_str(message)
    }
}


impl std::error::Error for MatchError {}


/// First qualifying contiguous horizontal run on the selected row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RunHit {
    /// Measured start in full-frame pixels, without calibration normalisation.
    pub anchor: PixelPoint,
    /// Full contiguous matching run length, including its final edge pixel.
    pub length: u32,
}


/// Optional constraint on the measured beginning of a horizontal run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RunStartConstraint {
    /// Caller-selected full-frame horizontal anchor.
    pub x: u32,
    /// Maximum inclusive distance from the measured start to that anchor.
    pub tolerance: u32,
}


/// First leftmost block whose entire probe-height columns satisfy the predicate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockHit {
    /// Measured top-left full-frame anchor.
    pub anchor: PixelPoint,
    /// Number of complete consecutive matching columns.
    pub width: u32,
    /// Entire supplied probe height covered by each matching column.
    pub height: u32,
}


/// Exact measured counts without assigning scene or completion meaning.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PixelCounts {
    /// Number of pixels satisfying the caller's RGB predicate.
    pub matching: u64,
    /// Total pixels in the validated ROI.
    pub total: u64,
}


impl PixelCounts {
    /// Return the matching fraction rounded down to per-mille precision.
    pub fn fraction_per_mille(self) -> u32 {
        if self.total == 0 {
            return 0;
        }

        ((u128::from(self.matching) * 1_000 / u128::from(self.total)).min(1_000)) as u32
    }


    /// Compare a per-mille threshold exactly, without rounding the measured ratio.
    pub fn meets_fraction_per_mille(self, minimum: u32) -> bool {
        self.total != 0 && minimum <= 1_000
            && u128::from(self.matching) * 1_000 >= u128::from(self.total) * u128::from(minimum)
    }
}


/// Match an RGB pixel against any palette entry within inclusive channel tolerance.
pub fn matches_palette(rgb: [u8; 3], palette: &[[u8; 3]], tolerance: u8) -> bool {
    palette.iter().any(|candidate| {
        rgb.into_iter().zip(candidate.iter()).all(|(channel, expected)| channel.abs_diff(*expected) <= tolerance)
    })
}


/// Safely validate an owned frame and read one RGB pixel without panicking.
pub fn pixel_rgb(frame: &CapturedFrame, x: u32, y: u32) -> Option<[u8; 3]> {
    frame.as_view().ok()?.pixel_rgb(x, y)
}


/// Find the first full qualifying run in top-to-bottom, then left-to-right order.
///
/// Minimum length is inclusive. A start constraint filters runs without moving
/// their returned anchors. Invalid frame/ROI/configuration is an error; an
/// otherwise valid ROI with no qualifying run returns `Ok(None)`.
pub fn find_horizontal_run<F>(
    frame: FrameView<'_>,
    bounds: PixelRect,
    minimum_length: u32,
    start_constraint: Option<RunStartConstraint>,
    predicate: F,
) -> Result<Option<RunHit>, MatchError>
where
    F: Fn([u8; 3]) -> bool,
{
    let (right, bottom) = frame.checked_bounds(bounds)?;

    if minimum_length == 0 {
        return Err(MatchError::ZeroMinimum);
    }

    for y in bounds.y..bottom {
        let mut run_start = bounds.x;
        let mut run_length = 0;

        for x in bounds.x..right {
            let rgb = frame.pixel_rgb(x, y).ok_or(MatchError::InvalidFrameLayout)?;

            if predicate(rgb) {
                if run_length == 0 {
                    run_start = x;
                }

                run_length += 1;
            } else {
                if qualifies_run(run_start, run_length, minimum_length, start_constraint) {
                    return Ok(Some(RunHit { anchor: checked_anchor(run_start, y)?, length: run_length }));
                }

                run_length = 0;
            }
        }

        if qualifies_run(run_start, run_length, minimum_length, start_constraint) {
            return Ok(Some(RunHit { anchor: checked_anchor(run_start, y)?, length: run_length }));
        }
    }

    Ok(None)
}


/// Count matching pixels in a checked ROI using widened exact arithmetic.
pub fn count_pixels<F>(frame: FrameView<'_>, bounds: PixelRect, predicate: F) -> Result<PixelCounts, MatchError>
where
    F: Fn([u8; 3]) -> bool,
{
    let (right, bottom) = frame.checked_bounds(bounds)?;
    let mut matching = 0_u64;

    for y in bounds.y..bottom {
        for x in bounds.x..right {
            let rgb = frame.pixel_rgb(x, y).ok_or(MatchError::InvalidFrameLayout)?;

            if predicate(rgb) {
                matching += 1;
            }
        }
    }

    Ok(PixelCounts { matching, total: u64::from(bounds.width) * u64::from(bounds.height) })
}


/// Find a leftmost contiguous block of complete predicate-matching columns.
///
/// Each accepted column covers the entire ROI height. Partial columns and
/// scattered matching pixels cannot qualify. Minimum width is inclusive.
pub fn find_solid_block<F>(
    frame: FrameView<'_>,
    bounds: PixelRect,
    minimum_width: u32,
    predicate: F,
) -> Result<Option<BlockHit>, MatchError>
where
    F: Fn([u8; 3]) -> bool,
{
    let (right, bottom) = frame.checked_bounds(bounds)?;

    if minimum_width == 0 {
        return Err(MatchError::ZeroMinimum);
    }

    let mut run_start = bounds.x;
    let mut run_width = 0;

    for x in bounds.x..right {
        let mut column_matches = true;

        for y in bounds.y..bottom {
            let rgb = frame.pixel_rgb(x, y).ok_or(MatchError::InvalidFrameLayout)?;

            if !predicate(rgb) {
                column_matches = false;
                break;
            }
        }

        if column_matches {
            if run_width == 0 {
                run_start = x;
            }

            run_width += 1;
        } else {
            if run_width >= minimum_width {
                return Ok(Some(BlockHit { anchor: checked_anchor(run_start, bounds.y)?, width: run_width, height: bounds.height }));
            }

            run_width = 0;
        }
    }

    if run_width >= minimum_width {
        return Ok(Some(BlockHit { anchor: checked_anchor(run_start, bounds.y)?, width: run_width, height: bounds.height }));
    }

    Ok(None)
}


/// Check inclusive length and optional inclusive measured-start tolerance.
fn qualifies_run(start: u32, length: u32, minimum: u32, constraint: Option<RunStartConstraint>) -> bool {
    length >= minimum && constraint.map(|constraint| start.abs_diff(constraint.x) <= constraint.tolerance).unwrap_or(true)
}


/// Convert measured unsigned anchors without narrowing or wrapping.
fn checked_anchor(x: u32, y: u32) -> Result<PixelPoint, MatchError> {
    let x = i32::try_from(x).map_err(|_| MatchError::AnchorOutsideCoordinateRange)?;
    let y = i32::try_from(y).map_err(|_| MatchError::AnchorOutsideCoordinateRange)?;
    Ok(PixelPoint::new(x, y))
}


#[cfg(test)]
mod tests {
    //! Synthetic checked-layout and scan boundary regressions without game fixtures.
    use super::*;


    /// Build a packed opaque RGBA frame with every RGB channel initially zero.
    fn frame(width: u32, height: u32) -> CapturedFrame {
        CapturedFrame {
            width, height, stride: width as usize * 4, format: PixelFormat::Rgba8,
            pixels: vec![0; width as usize * height as usize * 4],
        }
    }


    /// Set the red channel as a one-byte predicate marker in a synthetic frame.
    fn mark(frame: &mut CapturedFrame, x: u32, y: u32) {
        let offset = y as usize * frame.stride + x as usize * 4;
        frame.pixels[offset] = 255;
    }


    /// Reject short storage and overflowing host layout while supporting row padding.
    #[test]
    fn validates_layout_and_checks_pixel_coordinates() {
        let bytes = [1, 2, 3, 0, 9, 9, 9, 9, 4, 5, 6, 255, 9, 9, 9, 9];
        let view = FrameView::from_rgba(1, 2, 8, &bytes).unwrap();
        assert_eq!(view.pixel_rgb(0, 0), Some([1, 2, 3]));
        assert_eq!(view.pixel_rgb(0, 1), Some([4, 5, 6]));
        assert_eq!(view.pixel_rgb(1, 0), None);
        assert_eq!(view.pixel_rgb(0, 2), None);
        assert!(FrameView::from_rgba(1, 2, 8, &bytes[..15]).is_err());
        assert!(FrameView::from_rgba(1, 2, usize::MAX, &[]).is_err());
        assert!(FrameView::from_rgba(1, 1, 3, &[0; 4]).is_err());
    }


    /// Keep first-row/leftmost ordering, full lengths and inclusive start tolerance.
    #[test]
    fn run_search_preserves_measured_order_and_threshold_edges() {
        let mut frame = frame(12, 3);

        for x in 1..5 { mark(&mut frame, x, 0); }
        for x in 7..10 { mark(&mut frame, x, 0); }
        for x in 0..12 { mark(&mut frame, x, 1); }

        let view = frame.as_view().unwrap();
        let bounds = PixelRect::new(0, 0, 12, 3);
        assert_eq!(find_horizontal_run(view, bounds, 4, None, |rgb| rgb[0] == 255).unwrap(),
            Some(RunHit { anchor: PixelPoint::new(1, 0), length: 4 }));
        let constrained = Some(RunStartConstraint { x: 8, tolerance: 1 });
        assert_eq!(find_horizontal_run(view, bounds, 3, constrained, |rgb| rgb[0] == 255).unwrap(),
            Some(RunHit { anchor: PixelPoint::new(7, 0), length: 3 }));
        assert!(find_horizontal_run(view, bounds, 0, None, |_| true).is_err());
        assert_eq!(find_horizontal_run(view, PixelRect::new(10, 2, 2, 1), 1, None, |_| false).unwrap(), None);
    }


    /// Count exact ROI pixels, distinguish rounded ratios and reject invalid bounds.
    #[test]
    fn counts_use_exact_fraction_comparison_and_checked_bounds() {
        let mut frame = frame(3, 1);
        mark(&mut frame, 1, 0);
        let view = frame.as_view().unwrap();
        let counts = count_pixels(view, PixelRect::new(0, 0, 3, 1), |rgb| rgb[0] == 255).unwrap();
        assert_eq!(counts, PixelCounts { matching: 1, total: 3 });
        assert_eq!(counts.fraction_per_mille(), 333);
        assert!(counts.meets_fraction_per_mille(333));
        assert!(!counts.meets_fraction_per_mille(334));
        assert!(count_pixels(view, PixelRect::new(0, 0, 0, 1), |_| true).is_err());
        assert!(count_pixels(view, PixelRect::new(u32::MAX, 0, 2, 1), |_| true).is_err());
        assert!(count_pixels(view, PixelRect::new(2, 0, 2, 1), |_| true).is_err());
        assert_eq!(checked_anchor(u32::MAX, 0), Err(MatchError::AnchorOutsideCoordinateRange));
    }


    /// A block needs complete adjacent columns; an incomplete fringe cannot qualify.
    #[test]
    fn solid_block_requires_every_pixel_in_each_column() {
        let mut frame = frame(7, 2);
        mark(&mut frame, 0, 0);

        for x in 2..5 {
            mark(&mut frame, x, 0);
            mark(&mut frame, x, 1);
        }

        let view = frame.as_view().unwrap();
        let bounds = PixelRect::new(0, 0, 7, 2);
        assert_eq!(find_solid_block(view, bounds, 3, |rgb| rgb[0] == 255).unwrap(),
            Some(BlockHit { anchor: PixelPoint::new(2, 0), width: 3, height: 2 }));
        assert_eq!(find_solid_block(view, bounds, 4, |rgb| rgb[0] == 255).unwrap(), None);
        assert!(find_solid_block(view, bounds, 0, |_| true).is_err());
    }


    /// Palette tolerance is inclusive independently for each channel.
    #[test]
    fn palette_matching_checks_each_channel_without_fixed_game_colours() {
        assert!(matches_palette([10, 20, 30], &[[8, 22, 28]], 2));
        assert!(!matches_palette([10, 20, 30], &[[7, 22, 28]], 2));
        assert!(!matches_palette([10, 20, 30], &[], 255));
    }
}
