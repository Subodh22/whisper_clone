// Polls hardware key state directly from the OS HID layer.
// No event taps, no TSM calls, no special permissions needed for detection.

#[cfg(target_os = "macos")]
mod platform {
    // kCGEventSourceStateHIDSystemState = 1: raw hardware state, safe from any thread.
    // Replaces rdev which crashes on macOS 15 via TSMGetInputSourceProperty assertion.
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventSourceKeyState(state_id: i32, virtual_key: u16) -> bool;
    }

    fn key_down(vk: u16) -> bool {
        unsafe { CGEventSourceKeyState(1, vk) }
    }

    pub fn ctrl_held() -> bool {
        key_down(59) || key_down(62) // Left Control, Right Control
    }
    pub fn shift_held() -> bool {
        key_down(56) || key_down(60) // Left Shift, Right Shift
    }
    pub fn space_held() -> bool {
        key_down(49)
    }
    pub fn esc_held() -> bool {
        key_down(53)
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_ESCAPE, VK_LCONTROL, VK_LSHIFT, VK_RCONTROL, VK_RSHIFT, VK_SPACE,
    };

    fn key_down(vk: i32) -> bool {
        unsafe { GetAsyncKeyState(vk) as u16 & 0x8000 != 0 }
    }

    pub fn ctrl_held() -> bool {
        key_down(VK_LCONTROL as i32) || key_down(VK_RCONTROL as i32)
    }
    pub fn shift_held() -> bool {
        key_down(VK_LSHIFT as i32) || key_down(VK_RSHIFT as i32)
    }
    pub fn space_held() -> bool {
        key_down(VK_SPACE as i32)
    }
    pub fn esc_held() -> bool {
        key_down(VK_ESCAPE as i32)
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod platform {
    pub fn ctrl_held() -> bool { false }
    pub fn shift_held() -> bool { false }
    pub fn space_held() -> bool { false }
    pub fn esc_held() -> bool { false }
}

pub use platform::*;
