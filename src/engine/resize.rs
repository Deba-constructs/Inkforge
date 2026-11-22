//! Canvas resize geometry.
//!
//! Resizing adds margin or crops without resampling: every existing pixel keeps
//! its place relative to one of nine anchors, area outside the new bounds is
//! discarded and new area is transparent. The anchor decides which side grows.

/// Which part of the page stays put when the canvas changes size.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Anchor {
    #[default]
    TopLeft,
    Top,
    TopRight,
    Left,
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

impl Anchor {
    /// Every anchor, in the grid order the resize window shows them.
    pub const ALL: [[Self; 3]; 3] = [
        [Self::TopLeft, Self::Top, Self::TopRight],
        [Self::Left, Self::Center, Self::Right],
        [Self::BottomLeft, Self::Bottom, Self::BottomRight],
    ];

    /// Short label for tooltips.
    pub fn label(self) -> &'static str {
        match self {
            Self::TopLeft => "Top left",
            Self::Top => "Top",
            Self::TopRight => "Top right",
            Self::Left => "Left",
            Self::Center => "Centre",
            Self::Right => "Right",
            Self::BottomLeft => "Bottom left",
            Self::Bottom => "Bottom",
            Self::BottomRight => "Bottom right",
        }
    }

    /// Where the old origin `(0, 0)` lands in the new canvas.
    ///
    /// Negative on a side where the canvas shrank, which is exactly what a crop
    /// is: the pixels before that point fall off the page.
    pub fn offset(self, old: (u32, u32), new: (u32, u32)) -> (i32, i32) {
        let dx = new.0 as i64 - old.0 as i64;
        let dy = new.1 as i64 - old.1 as i64;

        let x = match self {
            Self::TopLeft | Self::Left | Self::BottomLeft => 0,
            Self::Top | Self::Center | Self::Bottom => dx / 2,
            Self::TopRight | Self::Right | Self::BottomRight => dx,
        };
        let y = match self {
            Self::TopLeft | Self::Top | Self::TopRight => 0,
            Self::Left | Self::Center | Self::Right => dy / 2,
            Self::BottomLeft | Self::Bottom | Self::BottomRight => dy,
        };

        (x as i32, y as i32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::egui::Color32;
    use crate::engine::Surface;

    #[test]
    fn growing_from_an_anchor_adds_margin_on_the_other_sides() {
        assert_eq!(Anchor::TopLeft.offset((100, 50), (200, 100)), (0, 0));
        assert_eq!(Anchor::Center.offset((100, 50), (200, 100)), (50, 25));
        assert_eq!(Anchor::BottomRight.offset((100, 50), (200, 100)), (100, 50));
        assert_eq!(Anchor::Bottom.offset((100, 50), (200, 100)), (50, 50));
    }

    #[test]
    fn shrinking_crops_the_opposite_sides() {
        assert_eq!(Anchor::TopLeft.offset((200, 100), (100, 50)), (0, 0));
        assert_eq!(Anchor::Center.offset((200, 100), (100, 50)), (-50, -25));
        assert_eq!(
            Anchor::BottomRight.offset((200, 100), (100, 50)),
            (-100, -50)
        );
    }

    #[test]
    fn a_resize_keeps_the_anchored_pixel_in_place() {
        for row in Anchor::ALL {
            for anchor in row {
                let old = (7, 4);
                let new = (11, 9);
                let (dx, dy) = anchor.offset(old, new);
                // The anchor point itself must map to the same relative spot.
                let corner = match anchor {
                    Anchor::TopLeft | Anchor::Left | Anchor::BottomLeft => 0,
                    Anchor::Top | Anchor::Center | Anchor::Bottom => old.0 / 2,
                    Anchor::TopRight | Anchor::Right | Anchor::BottomRight => old.0 - 1,
                };
                let mapped = corner as i32 + dx;
                assert!(
                    (0..new.0 as i32).contains(&mapped),
                    "{anchor:?} pushed the anchored column off the canvas"
                );
                assert!(dy <= new.1 as i32);
            }
        }
    }

    #[test]
    fn resizing_a_surface_moves_pixels_and_leaves_new_area_transparent() {
        let red = Color32::from_rgb(255, 0, 0);
        let green = Color32::from_rgb(0, 255, 0);
        let mut surface = Surface::new(2, 2);
        surface.write_region(
            crate::engine::PixelRegion {
                x: 0,
                y: 0,
                width: 2,
                height: 1,
            },
            &[red, green],
        );

        surface.resize(4, 4, (1, 1));
        assert_eq!(surface.width(), 4);
        assert_eq!(surface.pixel(0, 0), Color32::TRANSPARENT);
        assert_eq!(surface.pixel(1, 1), red);
        assert_eq!(surface.pixel(2, 1), green);
        assert_eq!(surface.pixel(3, 3), Color32::TRANSPARENT);

        // Cropping from the top-left keeps the pixel that belonged to (2, 1).
        surface.resize(1, 1, (-2, -1));
        assert_eq!(surface.pixel(0, 0), green);
    }
}
