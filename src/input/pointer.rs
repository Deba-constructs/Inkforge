//! The two Win32 pointer structures the pen path reads.
//!
//! `windows.h` is not available (no SDK on the build machine, and no dependency
//! on a bindings crate), so the layouts INKFORGE needs are declared here field
//! for field, in order, with the pointers and paddings that the C headers have
//! on 64-bit Windows. The unit tests below pin the size and the field offsets to
//! the documented SDK layout, so a mistyped field fails the build's test run
//! instead of corrupting memory on an artist's tablet.
//!
//! Only the fields the pen path uses are named in code; the rest still hold
//! their place so the offsets stay correct.

use std::ffi::c_void;

/// `POINT`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

/// `POINTER_INFO`. The first member of `POINTER_PEN_INFO`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PointerInfo {
    /// `POINTER_INPUT_TYPE` (an enum, so 32 bits).
    pub pointer_type: i32,
    pub pointer_id: u32,
    pub frame_id: u32,
    pub pointer_flags: u32,
    pub source_device: *mut c_void,
    pub hwnd_target: *mut c_void,
    pub pt_pixel_location: Point,
    pub pt_himetric_location: Point,
    pub pt_pixel_location_raw: Point,
    pub pt_himetric_location_raw: Point,
    pub time: u32,
    pub history_count: u32,
    pub input_state: i32,
    pub key_states: u32,
    pub performance_count: u64,
    /// `POINTER_BUTTON_CHANGE_TYPE` (an enum, so 32 bits).
    pub button_change_type: i32,
}

/// `POINTER_PEN_INFO`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PointerPenInfo {
    pub info: PointerInfo,
    /// `PEN_FLAGS` saying which of the fields below are valid.
    pub pen_mask: u32,
    /// `0..=1024`.
    pub pressure: u32,
    /// `0..=359` degrees.
    pub rotation: u32,
    /// `-90..=90` degrees.
    pub tilt_x: i32,
    /// `-90..=90` degrees.
    pub tilt_y: i32,
}

/// The pointer is in contact with the digitizer.
pub const POINTER_FLAG_INCONTACT: u32 = 0x0000_0004;
/// `pressure` is valid.
pub const PEN_MASK_PRESSURE: u32 = 0x0000_0001;
/// `tilt_x` is valid.
pub const PEN_MASK_TILT_X: u32 = 0x0000_0002;
/// `tilt_y` is valid.
pub const PEN_MASK_TILT_Y: u32 = 0x0000_0004;

/// Pen pressure is reported in this range.
pub const PRESSURE_MAX: f32 = 1024.0;

pub const WM_POINTERUPDATE: u32 = 0x0245;
pub const WM_POINTERDOWN: u32 = 0x0246;
pub const WM_POINTERUP: u32 = 0x0247;
pub const WM_POINTERLEAVE: u32 = 0x024A;
pub const WM_POINTERCAPTURECHANGED: u32 = 0x024C;

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{offset_of, size_of};

    /// The SDK layouts are for 64-bit Windows, so only 64-bit builds can check
    /// them (the release target is x86_64).
    #[test]
    #[cfg(target_pointer_width = "64")]
    fn pointer_structs_match_the_windows_layout() {
        assert_eq!(size_of::<Point>(), 8);
        assert_eq!(size_of::<PointerInfo>(), 96);
        assert_eq!(offset_of!(PointerInfo, pointer_flags), 12);
        assert_eq!(offset_of!(PointerInfo, hwnd_target), 24);
        assert_eq!(offset_of!(PointerInfo, pt_pixel_location), 32);
        assert_eq!(offset_of!(PointerInfo, time), 64);
        assert_eq!(offset_of!(PointerInfo, performance_count), 80);
        assert_eq!(offset_of!(PointerInfo, button_change_type), 88);

        assert_eq!(size_of::<PointerPenInfo>(), 120);
        assert_eq!(offset_of!(PointerPenInfo, pen_mask), 96);
        assert_eq!(offset_of!(PointerPenInfo, pressure), 100);
        assert_eq!(offset_of!(PointerPenInfo, rotation), 104);
        assert_eq!(offset_of!(PointerPenInfo, tilt_x), 108);
        assert_eq!(offset_of!(PointerPenInfo, tilt_y), 112);
    }

    /// A zeroed struct is what a failed call leaves behind: no contact, no
    /// pressure flag, so the pen path must fall back to a mouse reading.
    #[test]
    fn a_zeroed_reading_carries_no_pressure() {
        let info = PointerPenInfo::default();
        assert_eq!(info.info.pointer_flags & POINTER_FLAG_INCONTACT, 0);
        assert_eq!(info.pen_mask & PEN_MASK_PRESSURE, 0);
    }
}
