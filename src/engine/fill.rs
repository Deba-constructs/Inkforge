//! Flood fill: find the connected region of similar colour around a seed pixel.
//!
//! The caller supplies a sampling closure, so the same code can fill based on a
//! single layer or on the visible composite. The result is a mask plus its
//! bounding box so painting only touches the affected tiles.

use crate::egui::Color32;

use super::PixelRegion;

/// A filled region: `pixels` is row-major inside `bounds`.
pub struct FillMask {
    pub pixels: Vec<bool>,
    pub bounds: PixelRegion,
}

impl FillMask {
    /// Number of pixels the fill covers.
    pub fn count(&self) -> usize {
        self.pixels.iter().filter(|filled| **filled).count()
    }
}

/// Scanline flood fill from `start`, limited to `limit` (usually the selection).
///
/// `tolerance` is the largest per-channel difference (`0..=255`) still treated
/// as the same colour, measured on premultiplied channels — which is exactly
/// what the artist sees when the composite is sampled.
pub fn flood_mask<F>(
    sample: F,
    start: (u32, u32),
    tolerance: u8,
    width: u32,
    height: u32,
    limit: Option<PixelRegion>,
) -> Option<FillMask>
where
    F: Fn(u32, u32) -> Color32,
{
    if width == 0 || height == 0 {
        return None;
    }
    let (seed_x, seed_y) = start;
    if seed_x >= width || seed_y >= height {
        return None;
    }
    if let Some(limit) = limit {
        if seed_x < limit.x
            || seed_y < limit.y
            || seed_x >= limit.x + limit.width
            || seed_y >= limit.y + limit.height
        {
            return None;
        }
    }

    let seed = sample(seed_x, seed_y);
    let matches = |x: u32, y: u32| similar(seed, sample(x, y), tolerance);

    let mut filled = vec![false; width as usize * height as usize];
    let mut stack = vec![(seed_x, seed_y)];

    let in_limit = |x: u32, y: u32| match limit {
        Some(limit) => {
            x >= limit.x
                && y >= limit.y
                && x < limit.x + limit.width
                && y < limit.y + limit.height
        }
        None => true,
    };

    while let Some((x, y)) = stack.pop() {
        let row = y as usize * width as usize;
        if filled[row + x as usize] || !in_limit(x, y) || !matches(x, y) {
            continue;
        }

        // Expand left and right along this row, pushing the rows above and
        // below where the region continues.
        let mut left = x;
        while left > 0 && in_limit(left - 1, y) && !filled[row + (left - 1) as usize] && matches(left - 1, y) {
            left -= 1;
        }
        let mut right = x;
        while right + 1 < width
            && in_limit(right + 1, y)
            && !filled[row + (right + 1) as usize]
            && matches(right + 1, y)
        {
            right += 1;
        }

        for column in left..=right {
            filled[row + column as usize] = true;
        }

        for (neighbour_y, neighbour_row) in [
            (y.checked_sub(1), y.checked_sub(1).map(|ny| ny as usize * width as usize)),
            (y.checked_add(1).filter(|ny| *ny < height), y.checked_add(1).filter(|ny| *ny < height).map(|ny| ny as usize * width as usize)),
        ] {
            let (Some(ny), Some(nrow)) = (neighbour_y, neighbour_row) else {
                continue;
            };
            let mut column = left;
            while column <= right {
                if in_limit(column, ny) && !filled[nrow + column as usize] && matches(column, ny) {
                    stack.push((column, ny));
                    // Skip the rest of this run: one seed per span is enough.
                    while column <= right
                        && in_limit(column, ny)
                        && matches(column, ny)
                        && !filled[nrow + column as usize]
                    {
                        column += 1;
                    }
                } else {
                    column += 1;
                }
            }
        }
    }

    let count = filled.iter().filter(|value| **value).count();
    if count == 0 {
        return None;
    }

    // Trim to the filled bounding box so the mask is small.
    let mut min_x = width;
    let mut min_y = height;
    let mut max_x = 0;
    let mut max_y = 0;
    for y in 0..height {
        for x in 0..width {
            if filled[y as usize * width as usize + x as usize] {
                min_x = min_x.min(x);
                max_x = max_x.max(x);
                min_y = min_y.min(y);
                max_y = max_y.max(y);
            }
        }
    }

    let bounds = PixelRegion {
        x: min_x,
        y: min_y,
        width: max_x - min_x + 1,
        height: max_y - min_y + 1,
    };

    let mut pixels = Vec::with_capacity(bounds.pixel_count());
    for y in bounds.y..bounds.y + bounds.height {
        for x in bounds.x..bounds.x + bounds.width {
            pixels.push(filled[y as usize * width as usize + x as usize]);
        }
    }

    Some(FillMask { pixels, bounds })
}

/// True when every premultiplied channel is within `tolerance`.
fn similar(a: Color32, b: Color32, tolerance: u8) -> bool {
    let channels = [
        (a.r(), b.r()),
        (a.g(), b.g()),
        (a.b(), b.b()),
        (a.a(), b.a()),
    ];
    channels
        .iter()
        .all(|(x, y)| x.abs_diff(*y) <= tolerance as u8)
}
