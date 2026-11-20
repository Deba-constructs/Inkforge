//! Windows pen input: `WM_POINTER` messages on the app's own window.
//!
//! winit has no tablet API, so the window is subclassed with `SetWindowSubclass`
//! and the pen messages are read before winit sees them. The subclass always
//! ends with `DefSubclassProc`, which is also what promotes a pointer message
//! into the mouse messages winit expects — so installing this listener cannot
//! take the mouse away from the rest of the app.
//!
//! The reading is published through atomics and read once per frame by
//! [`InkforgeApp`](crate::app::InkforgeApp). There is no queue: a stroke only
//! needs the pressure the pen is at now, and the message handler runs on the
//! same thread as the frame, so the newest reading is the right one.

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use raw_window_handle::{HasWindowHandle, RawWindowHandle};

use super::pointer::{
    PointerPenInfo, PEN_MASK_PRESSURE, PEN_MASK_TILT_X, PEN_MASK_TILT_Y, POINTER_FLAG_INCONTACT,
    PRESSURE_MAX, WM_POINTERCAPTURECHANGED, WM_POINTERDOWN, WM_POINTERLEAVE, WM_POINTERUP,
    WM_POINTERUPDATE,
};
use crate::egui::Vec2;
use crate::engine::PenSample;

type Hwnd = *mut c_void;

/// Identity of our subclass, so it can be found again instead of stacked.
const SUBCLASS_ID: usize = 0x1;

#[link(name = "comctl32")]
extern "system" {
    fn SetWindowSubclass(
        hwnd: Hwnd,
        proc: unsafe extern "system" fn(Hwnd, u32, usize, isize, usize, usize) -> isize,
        id: usize,
        data: usize,
    ) -> i32;
    fn DefSubclassProc(hwnd: Hwnd, msg: u32, wparam: usize, lparam: isize) -> isize;
}

extern "system" {
    fn GetPointerPenInfo(pointer_id: u32, info: *mut PointerPenInfo) -> i32;
}

/// Pressure as `0.0..=1.0`, as raw bits.
static PRESSURE: AtomicU32 = AtomicU32::new(0);
/// Tilt in degrees, as raw bits.
static TILT_X: AtomicU32 = AtomicU32::new(0);
static TILT_Y: AtomicU32 = AtomicU32::new(0);
/// True while the pen is in contact and the numbers above are a real reading.
static IN_CONTACT: AtomicBool = AtomicBool::new(false);
static INSTALLED: AtomicBool = AtomicBool::new(false);

/// The latest reading: the pen's numbers while it touches the tablet, and a
/// mouse-equivalent sample otherwise (hovering, lifted, or no pen at all).
pub fn latest() -> PenSample {
    if !IN_CONTACT.load(Ordering::Relaxed) {
        return PenSample::MOUSE;
    }

    let pressure = f32::from_bits(PRESSURE.load(Ordering::Relaxed));
    let tilt = Vec2::new(
        f32::from_bits(TILT_X.load(Ordering::Relaxed)),
        f32::from_bits(TILT_Y.load(Ordering::Relaxed)),
    );
    PenSample::pen(pressure, tilt)
}

/// Install the pen listener on the window behind `window`.
pub fn attach(window: &impl HasWindowHandle) -> bool {
    if INSTALLED.load(Ordering::Relaxed) {
        return true;
    }

    let Ok(handle) = window.window_handle() else {
        return false;
    };
    let RawWindowHandle::Win32(win32) = handle.as_raw() else {
        return false;
    };

    // SAFETY: `hwnd` comes from the live window the app is rendering into, and
    // the subclass procedure outlives the window: it is a plain function with
    // no captured state.
    let installed = unsafe {
        SetWindowSubclass(
            win32.hwnd.get() as Hwnd,
            pen_subclass,
            SUBCLASS_ID,
            0,
        ) != 0
    };
    INSTALLED.store(installed, Ordering::Relaxed);
    installed
}

/// Message hook: copy the pen info out of pointer messages, then let the window
/// carry on with its normal handling.
unsafe extern "system" fn pen_subclass(
    hwnd: Hwnd,
    msg: u32,
    wparam: usize,
    lparam: isize,
    _id: usize,
    _data: usize,
) -> isize {
    match msg {
        // The pointer id is the low word of wParam (GET_POINTERID_WPARAM).
        WM_POINTERDOWN | WM_POINTERUPDATE => record((wparam & 0xFFFF) as u32),
        WM_POINTERUP | WM_POINTERLEAVE | WM_POINTERCAPTURECHANGED => {
            IN_CONTACT.store(false, Ordering::Relaxed);
        }
        _ => {}
    }

    // Always fall through: DefSubclassProc is what turns a pointer message into
    // the mouse input winit handles, so skipping it would break every tool.
    DefSubclassProc(hwnd, msg, wparam, lparam)
}

/// Read one pointer's pen info into the shared reading.
///
/// A mouse, a finger or a failed call leaves the shared reading alone; only a
/// pen in contact replaces it.
unsafe fn record(pointer_id: u32) {
    let mut info = PointerPenInfo::default();
    if GetPointerPenInfo(pointer_id, &mut info) == 0 {
        return;
    }

    if info.info.pointer_flags & POINTER_FLAG_INCONTACT == 0 {
        IN_CONTACT.store(false, Ordering::Relaxed);
        return;
    }

    let pressure = if info.pen_mask & PEN_MASK_PRESSURE != 0 {
        (info.pressure as f32 / PRESSURE_MAX).clamp(0.0, 1.0)
    } else {
        // The device reports contact but no pressure: paint at full size rather
        // than shrinking the brush to nothing.
        1.0
    };
    let tilt_x = if info.pen_mask & PEN_MASK_TILT_X != 0 {
        info.tilt_x as f32
    } else {
        0.0
    };
    let tilt_y = if info.pen_mask & PEN_MASK_TILT_Y != 0 {
        info.tilt_y as f32
    } else {
        0.0
    };

    PRESSURE.store(pressure.to_bits(), Ordering::Relaxed);
    TILT_X.store(tilt_x.to_bits(), Ordering::Relaxed);
    TILT_Y.store(tilt_y.to_bits(), Ordering::Relaxed);
    IN_CONTACT.store(true, Ordering::Relaxed);
}
