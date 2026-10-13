//! Early-exit horizontal black-pixel scans over a borrowed image rectangle.

use crate::geometry::{PixelPoint, PixelRect};

use super::{FrameView, MatchError, checked_anchor};


/// Find the first horizontal run of exact-black RGB pixels, scanning bottom-up.
///
/// The half-open `bounds` rectangle is a logical crop of `frame`: pixels remain
/// borrowed, and no image data is copied or allocated. Rows are visited from
/// bottom to top, and pixels within each row from left to right. Alpha is ignored.
///
/// Return the run's leftmost full-frame coordinate immediately when
/// `minimum_length` consecutive pixels have RGB `(0, 0, 0)`. A run never crosses
/// a row or the crop boundary. If no run qualifies, return `(0, 0)`.
///
/// The no-match sentinel is indistinguishable from a genuine hit at the image
/// origin. Callers using this sentinel must choose bounds that exclude `(0, 0)`.
///
/// # Errors
///
/// Reject empty or out-of-frame bounds, a zero minimum length, or a qualifying
/// coordinate that cannot be represented by [`PixelPoint`]. A minimum wider than
/// the crop is valid and produces the no-match sentinel.
pub fn find_black_run_bottom_up(
    frame: FrameView<'_>,
    bounds: PixelRect,
    minimum_length: u32,
) -> Result<PixelPoint, MatchError> {
    let (right, bottom) = frame.checked_bounds(bounds)?;

    if minimum_length == 0 {
        return Err(MatchError::ZeroMinimum);
    }

    if minimum_length > bounds.width {
        return Ok(PixelPoint::new(0, 0));
    }

    for y in (bounds.y..bottom).rev() {
        let mut run_start = bounds.x;
        let mut run_length = 0;

        for x in bounds.x..right {
            let rgb = frame.pixel_rgb(x, y).ok_or(MatchError::InvalidFrameLayout)?;

            if rgb == [0, 0, 0] {
                if run_length == 0 {
                    run_start = x;
                }

                run_length += 1;

                if run_length == minimum_length {
                    return checked_anchor(run_start, y);
                }
            } else {
                run_length = 0;
            }
        }
    }

    Ok(PixelPoint::new(0, 0))
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::image_matching::{CapturedFrame, PixelFormat};


    /// Construct a white image with caller-selected padding after every row.
    fn frame(width: u32, height: u32, padding: usize) -> CapturedFrame {
        let stride = width as usize * 4 + padding;
        CapturedFrame {
            width,
            height,
            stride,
            format: PixelFormat::Rgba8,
            pixels: vec![255; stride * height as usize],
        }
    }


    fn set_pixel(frame: &mut CapturedFrame, x: u32, y: u32, rgba: [u8; 4]) {
        let offset = y as usize * frame.stride + x as usize * 4;
        frame.pixels[offset..offset + 4].copy_from_slice(&rgba);
    }


    fn black_run(frame: &mut CapturedFrame, x: u32, y: u32, length: u32) {
        for column in x..x + length {
            set_pixel(frame, column, y, [0, 0, 0, 255]);
        }
    }


    #[test]
    fn bottom_row_precedes_upper_rows_and_leftmost_run_wins() {
        let mut frame = frame(24, 8, 0);
        black_run(&mut frame, 1, 2, 5);
        black_run(&mut frame, 6, 6, 5);
        black_run(&mut frame, 14, 6, 5);

        assert_eq!(
            find_black_run_bottom_up(frame.as_view().unwrap(), PixelRect::new(1, 1, 22, 6), 5),
            Ok(PixelPoint::new(6, 6)),
        );
    }


    #[test]
    fn requires_all_rgb_channels_to_be_zero_but_ignores_alpha() {
        let mut frame = frame(8, 6, 0);
        for (y, near_black) in [(5, [1, 0, 0, 255]), (4, [0, 1, 0, 255]), (3, [0, 0, 1, 255])] {
            black_run(&mut frame, 1, y, 5);
            set_pixel(&mut frame, 3, y, near_black);
        }
        for x in 1..6 {
            set_pixel(&mut frame, x, 2, [0, 0, 0, 0]);
        }

        assert_eq!(
            find_black_run_bottom_up(frame.as_view().unwrap(), PixelRect::new(1, 1, 6, 5), 5),
            Ok(PixelPoint::new(1, 2)),
        );
    }


    #[test]
    fn four_pixels_fail_and_five_or_more_return_the_run_start() {
        for (length, expected) in [(4, PixelPoint::new(0, 0)), (5, PixelPoint::new(3, 2)), (8, PixelPoint::new(3, 2))] {
            let mut frame = frame(12, 4, 0);
            black_run(&mut frame, 3, 2, length);
            assert_eq!(
                find_black_run_bottom_up(frame.as_view().unwrap(), PixelRect::new(1, 1, 10, 3), 5),
                Ok(expected),
            );
        }
    }


    #[test]
    fn respects_padding_without_treating_it_as_image_pixels() {
        let mut frame = frame(10, 4, 24);
        for row in 0..frame.height as usize {
            let padding_start = row * frame.stride + frame.width as usize * 4;
            let row_end = (row + 1) * frame.stride;
            frame.pixels[padding_start..row_end].fill(0);
        }
        black_run(&mut frame, 2, 1, 5);

        assert_eq!(
            find_black_run_bottom_up(frame.as_view().unwrap(), PixelRect::new(1, 1, 8, 3), 5),
            Ok(PixelPoint::new(2, 1)),
        );
    }


    #[test]
    fn does_not_join_rows_or_read_black_pixels_outside_the_crop() {
        let mut frame = frame(14, 7, 0);
        let bounds = PixelRect::new(3, 2, 8, 3);
        black_run(&mut frame, 3, 1, 8);
        black_run(&mut frame, 3, 5, 8);
        black_run(&mut frame, 1, 4, 6);
        black_run(&mut frame, 8, 3, 6);
        black_run(&mut frame, 3, 2, 2);

        assert_eq!(find_black_run_bottom_up(frame.as_view().unwrap(), bounds, 5), Ok(PixelPoint::new(0, 0)));
    }


    #[test]
    fn rejects_invalid_bounds_and_zero_minimum() {
        let frame = frame(10, 5, 0);
        let view = frame.as_view().unwrap();

        assert_eq!(find_black_run_bottom_up(view, PixelRect::new(1, 1, 0, 1), 5), Err(MatchError::EmptyBounds));
        assert_eq!(find_black_run_bottom_up(view, PixelRect::new(1, 1, 1, 0), 5), Err(MatchError::EmptyBounds));
        assert_eq!(find_black_run_bottom_up(view, PixelRect::new(6, 1, 5, 1), 5), Err(MatchError::BoundsOutsideFrame));
        assert_eq!(find_black_run_bottom_up(view, PixelRect::new(1, 4, 5, 2), 5), Err(MatchError::BoundsOutsideFrame));
        assert_eq!(find_black_run_bottom_up(view, PixelRect::new(u32::MAX, 1, 5, 1), 5), Err(MatchError::BoundsOutsideFrame));
        assert_eq!(find_black_run_bottom_up(view, PixelRect::new(1, u32::MAX, 5, 1), 5), Err(MatchError::BoundsOutsideFrame));
        assert_eq!(find_black_run_bottom_up(view, PixelRect::new(1, 1, 5, 1), 0), Err(MatchError::ZeroMinimum));
    }


    #[test]
    fn wider_minimum_and_an_all_white_crop_return_the_sentinel() {
        let frame = frame(10, 5, 0);
        let view = frame.as_view().unwrap();
        let bounds = PixelRect::new(1, 1, 8, 4);

        assert_eq!(find_black_run_bottom_up(view, bounds, 5), Ok(PixelPoint::new(0, 0)));
        assert_eq!(find_black_run_bottom_up(view, bounds, 9), Ok(PixelPoint::new(0, 0)));
    }
}
