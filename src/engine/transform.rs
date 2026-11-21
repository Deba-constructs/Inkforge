//! Whole-document image transforms.
//!
//! Flipping and rotating the artwork is a lossless permutation of pixels: every
//! source pixel lands on exactly one destination pixel and nothing is resampled,
//! so a transform followed by its [`Transform::inverse`] returns the document to
//! the byte. That is also how undo works — the history stores the transform, not
//! a copy of every layer, and reverses it with the opposite transform.

/// A lossless transform applied to every layer of a document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transform {
    FlipHorizontal,
    FlipVertical,
    Rotate90Cw,
    Rotate90Ccw,
    Rotate180,
}

impl Transform {
    /// Every transform, in the order the Image menu lists them.
    pub const ALL: [Self; 5] = [
        Self::FlipHorizontal,
        Self::FlipVertical,
        Self::Rotate90Cw,
        Self::Rotate90Ccw,
        Self::Rotate180,
    ];

    /// Menu wording.
    pub fn label(self) -> &'static str {
        match self {
            Self::FlipHorizontal => "Flip horizontal",
            Self::FlipVertical => "Flip vertical",
            Self::Rotate90Cw => "Rotate 90° clockwise",
            Self::Rotate90Ccw => "Rotate 90° counter-clockwise",
            Self::Rotate180 => "Rotate 180°",
        }
    }

    /// Undo wording: the edit that reverses this one.
    pub fn undo_label(self) -> &'static str {
        match self {
            Self::FlipHorizontal => "flip horizontal",
            Self::FlipVertical => "flip vertical",
            Self::Rotate90Cw => "rotate 90° clockwise",
            Self::Rotate90Ccw => "rotate 90° counter-clockwise",
            Self::Rotate180 => "rotate 180°",
        }
    }

    /// The transform that undoes this one.
    pub fn inverse(self) -> Self {
        match self {
            Self::FlipHorizontal => Self::FlipHorizontal,
            Self::FlipVertical => Self::FlipVertical,
            Self::Rotate90Cw => Self::Rotate90Ccw,
            Self::Rotate90Ccw => Self::Rotate90Cw,
            Self::Rotate180 => Self::Rotate180,
        }
    }

    /// Canvas size after the transform.
    pub fn output_size(self, width: u32, height: u32) -> (u32, u32) {
        match self {
            Self::FlipHorizontal | Self::FlipVertical | Self::Rotate180 => (width, height),
            Self::Rotate90Cw | Self::Rotate90Ccw => (height, width),
        }
    }

    /// Source pixel for destination `(x, y)` of a `width × height` image.
    ///
    /// Always in bounds for a destination inside [`Self::output_size`].
    pub fn source_pixel(self, x: u32, y: u32, width: u32, height: u32) -> (u32, u32) {
        match self {
            Self::FlipHorizontal => (width - 1 - x, y),
            Self::FlipVertical => (x, height - 1 - y),
            Self::Rotate180 => (width - 1 - x, height - 1 - y),
            // A clockwise turn sends the left column to the top row, so the
            // destination row reads the source column from the bottom up.
            Self::Rotate90Cw => (y, height - 1 - x),
            // Counter-clockwise sends the right column to the top row.
            Self::Rotate90Ccw => (width - 1 - y, x),
        }
    }

}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_transform_stays_inside_the_image() {
        for transform in Transform::ALL {
            let (width, height) = (5, 3);
            let (out_width, out_height) = transform.output_size(width, height);
            let mut seen = vec![false; (width * height) as usize];

            for y in 0..out_height {
                for x in 0..out_width {
                    let (sx, sy) = transform.source_pixel(x, y, width, height);
                    assert!(
                        sx < width && sy < height,
                        "{transform:?} mapped ({x}, {y}) outside the image"
                    );
                    let slot = (sy * width + sx) as usize;
                    assert!(
                        !seen[slot],
                        "{transform:?} read source pixel ({sx}, {sy}) twice"
                    );
                    seen[slot] = true;
                }
            }

            assert!(
                seen.iter().all(|hit| *hit),
                "{transform:?} left some source pixels unread, so it is not a permutation"
            );
        }
    }

    #[test]
    fn rotating_twice_in_the_same_direction_is_a_half_turn() {
        let (width, height) = (4, 7);
        let (w1, h1) = Transform::Rotate90Cw.output_size(width, height);
        let (w2, h2) = Transform::Rotate90Cw.output_size(w1, h1);
        assert_eq!((w2, h2), (width, height), "two turns give the original size");

        for y in 0..h2 {
            for x in 0..w2 {
                let (mx, my) = Transform::Rotate90Cw.source_pixel(x, y, w1, h1);
                let (sx, sy) = Transform::Rotate90Cw.source_pixel(mx, my, width, height);
                let expected = Transform::Rotate180.source_pixel(x, y, width, height);
                assert_eq!((sx, sy), expected, "two turns equal a half turn at ({x}, {y})");
            }
        }
    }

    #[test]
    fn clockwise_and_counter_clockwise_are_mirror_images() {
        let (width, height) = (6, 2);
        let mut clockwise: Vec<(u32, u32)> = Vec::new();
        let mut counter: Vec<(u32, u32)> = Vec::new();
        let (out_width, out_height) = Transform::Rotate90Cw.output_size(width, height);

        for y in 0..out_height {
            for x in 0..out_width {
                clockwise.push(Transform::Rotate90Cw.source_pixel(x, y, width, height));
                counter.push(Transform::Rotate90Ccw.source_pixel(x, y, width, height));
            }
        }

        clockwise.sort_unstable();
        counter.sort_unstable();
        assert_eq!(
            clockwise, counter,
            "both turns must cover exactly the same source pixels"
        );
    }

}
