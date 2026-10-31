//! Offline whole-comparison timing for a 31-by-49 packed rank/suit region.
//!
//! Input preparation and result printing occur outside the single timed call.
//! This is one local observation, not a benchmark distribution or latency bound.

use std::{hint::black_box, time::Instant};

use xsar::image_matching::packed_comparison::compare_packed_bytes;


/// Time only one complete safe comparison, with no timers inside its loop.
fn main() {
    let before = vec![u32::from_le_bytes([240, 220, 200, 255]); 31 * 49];
    let after = vec![u32::from_le_bytes([20, 30, 40, 255]); 31 * 49];
    let before = black_box(before.as_slice());
    let after = black_box(after.as_slice());
    let started = Instant::now();
    let result = compare_packed_bytes(before, after, 12);
    let elapsed = started.elapsed();
    let result = black_box(result.unwrap());
    println!(
        "{} words: whole comparison elapsed {elapsed:?}; RGB changed={:?}; RGB absolute sums={:?}",
        result.words,
        &result.changed[..3],
        &result.absolute_difference[..3]
    );
}
