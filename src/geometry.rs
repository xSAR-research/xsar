//! Checked conversion between guest-pixel geometry and QMP absolute coordinates.

use std::fmt;


/// Maximum value of QEMU's absolute pointing-device coordinate range.
pub const QMP_ABSOLUTE_MAX: u32 = 0x7fff;


/// Signed guest-pixel point; negative values remain detectable before input.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PixelPoint {
    /// Horizontal coordinate, measured from the left edge.
    pub x: i32,
    /// Vertical coordinate, measured from the top edge.
    pub y: i32,
}


impl PixelPoint {


    /// Construct a signed guest-pixel point.
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}


/// Half-open guest-pixel rectangle with unsigned origin and dimensions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PixelRect {
    /// Horizontal coordinate, measured from the left edge.
    pub x: u32,
    /// Vertical coordinate, measured from the top edge.
    pub y: u32,
    /// Rectangle width in guest pixels.
    pub width: u32,
    /// Rectangle height in guest pixels.
    pub height: u32,
}


impl PixelRect {


    /// Construct a half-open guest-pixel rectangle.
    pub const fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }


    /// Return the saturating exclusive right edge.
    pub const fn right(self) -> u32 {
        self.x.saturating_add(self.width)
    }


    /// Return the saturating exclusive bottom edge.
    pub const fn bottom(self) -> u32 {
        self.y.saturating_add(self.height)
    }


    /// Report whether either rectangle dimension is zero.
    pub const fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }


    /// Return the integer midpoint when both coordinates fit a signed point.
    ///
    /// Returns `None` if midpoint arithmetic overflows `u32` or either midpoint
    /// exceeds `i32::MAX`. Empty rectangles retain their origin as the midpoint
    /// on each empty axis. This does not validate the full rectangle or a frame.
    pub const fn try_centre(self) -> Option<PixelPoint> {
        let x = match self.x.checked_add(self.width / 2) {
            Some(x) => x,
            None => return None,
        };
        let y = match self.y.checked_add(self.height / 2) {
            Some(y) => y,
            None => return None,
        };


        if x > i32::MAX as u32 || y > i32::MAX as u32 {
            return None;
        }

        Some(PixelPoint::new(x as i32, y as i32))
    }


    /// Return the integer midpoint used by calibrated screen targets.
    ///
    /// # Panics
    ///
    /// Panics if midpoint arithmetic overflows or the midpoint does not fit
    /// `PixelPoint`. Use [`Self::try_centre`] for caller-supplied geometry.
    pub const fn centre(self) -> PixelPoint {
        match self.try_centre() {
            Some(point) => point,
            None => panic!("pixel rectangle midpoint does not fit a signed pixel point"),
        }
    }


    /// Check membership using the rectangle's half-open pixel bounds.
    ///
    /// Negative points and rectangles with overflowing exclusive edges are
    /// rejected. Unsigned rectangle coordinates are never narrowed to `i32`.
    pub fn contains(self, point: PixelPoint) -> bool {
        let (Ok(x), Ok(y)) = (u32::try_from(point.x), u32::try_from(point.y)) else {
            return false;
        };
        let (Some(right), Some(bottom)) = (
            self.x.checked_add(self.width),
            self.y.checked_add(self.height),
        ) else {
            return false;
        };

        x >= self.x && y >= self.y && x < right && y < bottom
    }
}


/// Absolute pointing-device coordinate in QEMU's calibrated input range.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct QmpPoint {
    /// Horizontal coordinate, measured from the left edge.
    pub x: u32,
    /// Vertical coordinate, measured from the top edge.
    pub y: u32,
}


impl QmpPoint {


    /// Construct an absolute QMP point.
    pub const fn new(x: u32, y: u32) -> Self {
        Self { x, y }
    }
}


/// Inclusive QMP corners corresponding to a half-open pixel rectangle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct QmpRect {
    /// QMP coordinate of the first included pixel.
    pub top_left: QmpPoint,
    /// QMP coordinate of the final pixel inside the half-open source rectangle.
    pub bottom_right_inclusive: QmpPoint,
}


impl QmpRect {


    /// Construct inclusive QMP corner geometry.
    pub const fn new(top_left: QmpPoint, bottom_right_inclusive: QmpPoint) -> Self {
        Self {
            top_left,
            bottom_right_inclusive,
        }
    }
}


/// Rejection reasons for coordinates that cannot map safely into the frame.
#[derive(Debug, PartialEq, Eq)]
pub enum GeometryError {
    /// The axis contains no pixels.
    InvalidExtent,
    /// The unsigned coordinate is beyond the declared axis extent.
    PixelOutsideExtent {
        /// Rejected unsigned pixel coordinate.
        pixel: u32,
        /// Declared number of pixels on this axis.
        extent: u32,
    },
    /// The signed point is negative or beyond the declared frame.
    PixelPointOutsideFrame {
        /// Rejected horizontal origin or point coordinate.
        x: i32,
        /// Rejected vertical origin or point coordinate.
        y: i32,
        /// Declared guest image width.
        frame_width: u32,
        /// Declared guest image height.
        frame_height: u32,
    },
    /// At least one rectangle dimension is zero.
    EmptyRectangle,
    /// Computing an exclusive rectangle edge overflows u32.
    RectangleOverflow,
    /// The rectangle extends beyond the declared frame.
    RectangleOutsideFrame {
        /// Rejected horizontal origin or point coordinate.
        x: u32,
        /// Rejected vertical origin or point coordinate.
        y: u32,
        /// Rejected rectangle width in pixels.
        width: u32,
        /// Rejected rectangle height in pixels.
        height: u32,
        /// Declared guest image width.
        frame_width: u32,
        /// Declared guest image height.
        frame_height: u32,
    },
}


impl fmt::Display for GeometryError {


    /// Describe the rejected coordinate or rectangle without extra dependencies.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidExtent => write!(formatter, "axis extent must be non-zero"),
            Self::PixelOutsideExtent { pixel, extent } => {
                write!(formatter, "pixel coordinate {pixel} is outside extent {extent}")
            }
            Self::PixelPointOutsideFrame { x, y, frame_width, frame_height } => {
                write!(formatter,
                    "pixel point ({x}, {y}) is outside frame {frame_width}x{frame_height}")
            }
            Self::EmptyRectangle => write!(formatter, "pixel rectangle must be non-empty"),
            Self::RectangleOverflow => {
                write!(formatter, "pixel rectangle overflows the u32 coordinate space")
            }
            Self::RectangleOutsideFrame { x, y, width, height, frame_width, frame_height } => {
                write!(formatter,
                    "pixel rectangle ({x}, {y}, {width}, {height}) is outside frame {frame_width}x{frame_height}")
            }
        }
    }
}


impl std::error::Error for GeometryError {}


/// Convert one guest pixel coordinate to QEMU's absolute input range.
pub fn pixel_to_qmp_axis(pixel: u32, extent: u32) -> Result<u32, GeometryError> {


    if extent == 0 {
        return Err(GeometryError::InvalidExtent);
    }


    if pixel >= extent {
        return Err(GeometryError::PixelOutsideExtent { pixel, extent });
    }

    let numerator = u64::from(pixel) * u64::from(QMP_ABSOLUTE_MAX);
    Ok((numerator / u64::from(extent)) as u32)
}


/// Convert one guest pixel point to QEMU's absolute input coordinates.
pub fn pixel_point_to_qmp(
    point: PixelPoint,
    frame_width: u32,
    frame_height: u32,
) -> Result<QmpPoint, GeometryError> {


    if point.x < 0 || point.y < 0 || point.x as u32 >= frame_width || point.y as u32 >= frame_height
    {
        return Err(GeometryError::PixelPointOutsideFrame {
            x: point.x,
            y: point.y,
            frame_width,
            frame_height,
        });
    }

    Ok(QmpPoint::new(
        pixel_to_qmp_axis(point.x as u32, frame_width)?,
        pixel_to_qmp_axis(point.y as u32, frame_height)?,
    ))
}


/// Convert a half-open pixel rectangle to inclusive QMP corner coordinates.
pub fn pixel_rect_to_qmp(
    rect: PixelRect,
    frame_width: u32,
    frame_height: u32,
) -> Result<QmpRect, GeometryError> {


    if rect.is_empty() {
        return Err(GeometryError::EmptyRectangle);
    }

    let right = rect
        .x
        .checked_add(rect.width)
        .ok_or(GeometryError::RectangleOverflow)?;
    let bottom = rect
        .y
        .checked_add(rect.height)
        .ok_or(GeometryError::RectangleOverflow)?;


    if right > frame_width || bottom > frame_height {
        return Err(GeometryError::RectangleOutsideFrame {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
            frame_width,
            frame_height,
        });
    }

    let top_left = QmpPoint::new(
        pixel_to_qmp_axis(rect.x, frame_width)?,
        pixel_to_qmp_axis(rect.y, frame_height)?,
    );
    let bottom_right_inclusive = QmpPoint::new(
        pixel_to_qmp_axis(right - 1, frame_width)?,
        pixel_to_qmp_axis(bottom - 1, frame_height)?,
    );

    Ok(QmpRect::new(top_left, bottom_right_inclusive))
}


#[cfg(test)]
mod tests {
    //! Calibrated conversions and coordinate-boundary regression coverage.
    use super::*;


    /// Keep calibrated and empty midpoint behavior while checking signed limits.
    #[test]
    fn midpoint_is_checked_without_breaking_calibrated_centres() {
        let rect = PixelRect::new(1_655, 443, 137, 181);
        assert_eq!(rect.try_centre(), Some(PixelPoint::new(1_723, 533)));
        assert_eq!(rect.centre(), PixelPoint::new(1_723, 533));
        assert_eq!(PixelRect::new(12, 34, 0, 0).centre(), PixelPoint::new(12, 34));
        assert_eq!(
            PixelRect::new(i32::MAX as u32, 0, 1, 1).try_centre(),
            Some(PixelPoint::new(i32::MAX, 0))
        );
        assert_eq!(PixelRect::new(i32::MAX as u32, 0, 2, 1).try_centre(), None);
        assert_eq!(PixelRect::new(0, i32::MAX as u32 + 1, 1, 1).try_centre(), None);
        assert_eq!(PixelRect::new(u32::MAX, 0, 2, 1).try_centre(), None);
        assert_eq!(PixelRect::new(0, u32::MAX, 1, 2).try_centre(), None);
    }


    /// The compatibility midpoint helper must refuse unrepresentable targets.
    #[test]
    #[should_panic(expected = "pixel rectangle midpoint does not fit a signed pixel point")]
    fn compatibility_centre_panics_instead_of_wrapping() {
        PixelRect::new(i32::MAX as u32 + 1, 0, 1, 1).centre();
    }


    /// Half-open membership uses unsigned bounds without wrapping large origins.
    #[test]
    fn membership_checks_signed_points_and_overflowing_edges() {
        let rect = PixelRect::new(10, 20, 2, 3);
        assert!(rect.contains(PixelPoint::new(10, 20)));
        assert!(rect.contains(PixelPoint::new(11, 22)));
        assert!(!rect.contains(PixelPoint::new(12, 22)));
        assert!(!rect.contains(PixelPoint::new(11, 23)));
        assert!(!rect.contains(PixelPoint::new(-1, 20)));
        assert!(!rect.contains(PixelPoint::new(10, -1)));
        assert!(!PixelRect::new(10, 20, 0, 3).contains(PixelPoint::new(10, 20)));
        assert!(!PixelRect::new(10, 20, 2, 0).contains(PixelPoint::new(10, 20)));
        assert!(!PixelRect::new(i32::MAX as u32 + 1, 0, 1, 1)
            .contains(PixelPoint::new(i32::MIN, 0)));
        assert!(PixelRect::new(i32::MAX as u32, 0, 1, 1)
            .contains(PixelPoint::new(i32::MAX, 0)));
        assert!(!PixelRect::new(0, 0, u32::MAX, u32::MAX)
            .contains(PixelPoint::new(-1, -1)));
        assert!(!PixelRect::new(1, 0, u32::MAX, 1)
            .contains(PixelPoint::new(1, 0)));
        assert!(!PixelRect::new(0, 1, 1, u32::MAX)
            .contains(PixelPoint::new(0, 1)));
    }


    /// Preserve the established pixel-to-QMP calibration vectors.
    #[test]
    fn matches_audited_calibration_vectors() {
        // QEMU's tablet range is scaled with floor(pixel * 32767 / extent).
        let cases = [
            (0, 1_920, 0),
            (1, 1_920, 17),
            (400, 1_920, 6_826),
            (960, 1_920, 16_383),
            (1_723, 1_920, 29_404),
            (1_919, 1_920, 32_749),
            (1, 1_080, 30),
            (533, 1_080, 16_171),
            (540, 1_080, 16_383),
            (800, 1_080, 24_271),
            (1_079, 1_080, 32_736),
        ];


        for (pixel, extent, expected) in cases {
            assert_eq!(pixel_to_qmp_axis(pixel, extent), Ok(expected));
        }
    }


    /// Cover the smallest extent and u32 limits without intermediate overflow.
    #[test]
    fn accepts_one_pixel_extent_and_uses_widened_arithmetic() {
        assert_eq!(pixel_to_qmp_axis(0, 1), Ok(0));
        assert_eq!(pixel_to_qmp_axis(u32::MAX - 1, u32::MAX), Ok(32_766));
    }


    /// Reject empty axes and coordinates at the exclusive frame edge.
    #[test]
    fn rejects_zero_extent_and_coordinates_outside_extent() {
        assert_eq!(pixel_to_qmp_axis(0, 0), Err(GeometryError::InvalidExtent));
        assert_eq!(
            pixel_to_qmp_axis(1920, 1920),
            Err(GeometryError::PixelOutsideExtent {
                pixel: 1920,
                extent: 1920,
            })
        );
    }


    /// Verify monotonic conversion against the calibrated integer formula.
    #[test]
    fn mapping_is_monotone_and_matches_the_integer_formula() {
        let mut previous = 0;


        for pixel in 0..1_920 {
            let actual = pixel_to_qmp_axis(pixel, 1_920).unwrap();
            let expected = (u64::from(pixel) * u64::from(QMP_ABSOLUTE_MAX) / 1_920) as u32;
            assert_eq!(actual, expected);
            assert!(actual >= previous);
            assert!(actual < QMP_ABSOLUTE_MAX);
            previous = actual;
        }
    }


    /// Verify inclusive QMP corners use the final included source pixel.
    #[test]
    fn converts_half_open_rect_using_its_last_included_pixel() {
        let rect = PixelRect::new(1_655, 443, 137, 181);
        let converted = pixel_rect_to_qmp(rect, 1_920, 1_080).unwrap();

        assert_eq!(converted.top_left, QmpPoint::new(28_244, 13_440));
        assert_eq!(
            converted.bottom_right_inclusive,
            QmpPoint::new(30_565, 18_901)
        );
    }


    /// Exercise empty, overflowing and frame-crossing rectangle failures.
    #[test]
    fn rejects_empty_overflowing_and_out_of_frame_rectangles() {
        assert_eq!(
            pixel_rect_to_qmp(PixelRect::new(10, 10, 0, 1), 1_920, 1_080),
            Err(GeometryError::EmptyRectangle)
        );
        assert_eq!(
            pixel_rect_to_qmp(PixelRect::new(u32::MAX, 10, 2, 1), 1_920, 1_080),
            Err(GeometryError::RectangleOverflow)
        );
        assert_eq!(
            pixel_rect_to_qmp(PixelRect::new(1_900, 1_070, 21, 10), 1_920, 1_080),
            Err(GeometryError::RectangleOutsideFrame {
                x: 1_900,
                y: 1_070,
                width: 21,
                height: 10,
                frame_width: 1_920,
                frame_height: 1_080,
            })
        );
    }


    /// Ensure large unsigned rectangle origins are not narrowed through i32.
    #[test]
    fn rectangle_conversion_does_not_narrow_u32_coordinates_to_i32() {
        let rect = PixelRect::new(i32::MAX as u32 + 1, 0, 1, 1);
        let converted = pixel_rect_to_qmp(rect, u32::MAX, 1).unwrap();
        let expected_x = pixel_to_qmp_axis(rect.x, u32::MAX).unwrap();

        assert_eq!(converted.top_left, QmpPoint::new(expected_x, 0));
        assert_eq!(
            converted.bottom_right_inclusive,
            QmpPoint::new(expected_x, 0)
        );
    }
}
