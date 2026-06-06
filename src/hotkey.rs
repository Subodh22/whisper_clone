// Polls hardware key state directly from the OS HID layer.
// No event taps, no TSM calls, no special permissions needed for detection.

/// Parsed hotkey definition — platform-agnostic modifiers + one key code.
#[derive(Debug, Clone)]
pub struct HotkeyDef {
    pub needs_ctrl: bool,
    pub needs_shift: bool,
    pub needs_alt: bool,
    pub needs_cmd: bool, // macOS: Cmd key; Windows: Win key
    pub key_code: u16,   // platform-specific keycode
}

/// Parse a hotkey string like "ctrl+shift+space" or "cmd+shift+d".
/// Unknown keys fall back to Space. Unknown modifiers are ignored.
pub fn parse_hotkey(s: &str) -> HotkeyDef {
    let mut def = HotkeyDef {
        needs_ctrl: false,
        needs_shift: false,
        needs_alt: false,
        needs_cmd: false,
        key_code: platform::KEY_SPACE,
    };
    for part in s.to_lowercase().split('+') {
        match part.trim() {
            "ctrl" | "control" => def.needs_ctrl = true,
            "shift" => def.needs_shift = true,
            "alt" | "option" | "opt" => def.needs_alt = true,
            "cmd" | "command" | "meta" | "super" | "win" | "windows" => def.needs_cmd = true,
            key => {
                if let Some(code) = platform::key_name_to_code(key) {
                    def.key_code = code;
                }
            }
        }
    }
    def
}

/// Returns true if all modifiers and the main key in `def` are currently held.
pub fn hotkey_active(def: &HotkeyDef) -> bool {
    platform::hotkey_active_impl(def)
}

/// Returns true if the Escape key is currently held.
pub fn esc_held() -> bool {
    platform::esc_held_impl()
}

// ───────────────────────────── macOS ─────────────────────────────
#[cfg(target_os = "macos")]
mod platform {
    use super::HotkeyDef;

    // kCGEventSourceStateHIDSystemState = 1: raw hardware state, safe from any thread.
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventSourceKeyState(state_id: i32, virtual_key: u16) -> bool;
    }

    fn key_down(vk: u16) -> bool {
        unsafe { CGEventSourceKeyState(1, vk) }
    }

    pub const KEY_SPACE: u16 = 49;

    pub fn key_name_to_code(name: &str) -> Option<u16> {
        Some(match name {
            "space" => 49,
            "a" => 0, "s" => 1, "d" => 2, "f" => 3, "g" => 5, "h" => 4,
            "j" => 38, "k" => 40, "l" => 37,
            "z" => 6, "x" => 7, "c" => 8, "v" => 9, "b" => 11,
            "n" => 45, "m" => 46,
            "q" => 12, "w" => 13, "e" => 14, "r" => 15, "t" => 17,
            "y" => 16, "u" => 32, "i" => 34, "o" => 31, "p" => 35,
            "1" => 18, "2" => 19, "3" => 20, "4" => 21, "5" => 23,
            "6" => 22, "7" => 26, "8" => 28, "9" => 25, "0" => 29,
            "enter" | "return" => 36,
            "tab" => 48,
            "backspace" | "delete" => 51,
            "esc" | "escape" => 53,
            _ => return None,
        })
    }

    pub fn hotkey_active_impl(def: &HotkeyDef) -> bool {
        // Left/right variants for each modifier
        let ctrl_ok = !def.needs_ctrl || key_down(59) || key_down(62);
        let shift_ok = !def.needs_shift || key_down(56) || key_down(60);
        let alt_ok = !def.needs_alt || key_down(58) || key_down(61);
        let cmd_ok = !def.needs_cmd || key_down(55) || key_down(54);
        let key_ok = key_down(def.key_code);
        ctrl_ok && shift_ok && alt_ok && cmd_ok && key_ok
    }

    pub fn esc_held_impl() -> bool {
        key_down(53)
    }
}

// ───────────────────────────── Windows ─────────────────────────────
#[cfg(target_os = "windows")]
mod platform {
    use super::HotkeyDef;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_ESCAPE, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN,
        VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SPACE,
    };

    fn key_down(vk: i32) -> bool {
        unsafe { GetAsyncKeyState(vk) as u16 & 0x8000 != 0 }
    }

    pub const KEY_SPACE: u16 = VK_SPACE as u16;

    pub fn key_name_to_code(name: &str) -> Option<u16> {
        Some(match name {
            "space" => VK_SPACE as u16,
            "a" => 0x41, "b" => 0x42, "c" => 0x43, "d" => 0x44, "e" => 0x45,
            "f" => 0x46, "g" => 0x47, "h" => 0x48, "i" => 0x49, "j" => 0x4A,
            "k" => 0x4B, "l" => 0x4C, "m" => 0x4D, "n" => 0x4E, "o" => 0x4F,
            "p" => 0x50, "q" => 0x51, "r" => 0x52, "s" => 0x53, "t" => 0x54,
            "u" => 0x55, "v" => 0x56, "w" => 0x57, "x" => 0x58, "y" => 0x59,
            "z" => 0x5A,
            "0" => 0x30, "1" => 0x31, "2" => 0x32, "3" => 0x33, "4" => 0x34,
            "5" => 0x35, "6" => 0x36, "7" => 0x37, "8" => 0x38, "9" => 0x39,
            "esc" | "escape" => VK_ESCAPE as u16,
            _ => return None,
        })
    }

    pub fn hotkey_active_impl(def: &HotkeyDef) -> bool {
        let ctrl_ok = !def.needs_ctrl
            || key_down(VK_LCONTROL as i32)
            || key_down(VK_RCONTROL as i32);
        let shift_ok = !def.needs_shift
            || key_down(VK_LSHIFT as i32)
            || key_down(VK_RSHIFT as i32);
        let alt_ok = !def.needs_alt
            || key_down(VK_LMENU as i32)
            || key_down(VK_RMENU as i32);
        let cmd_ok = !def.needs_cmd
            || key_down(VK_LWIN as i32)
            || key_down(VK_RWIN as i32);
        let key_ok = key_down(def.key_code as i32);
        ctrl_ok && shift_ok && alt_ok && cmd_ok && key_ok
    }

    pub fn esc_held_impl() -> bool {
        key_down(VK_ESCAPE as i32)
    }
}

// ───────────────────────────── Other platforms ─────────────────────────────
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod platform {
    use super::HotkeyDef;
    pub const KEY_SPACE: u16 = 49;
    pub fn key_name_to_code(_: &str) -> Option<u16> { None }
    pub fn hotkey_active_impl(_: &HotkeyDef) -> bool { false }
    pub fn esc_held_impl() -> bool { false }
}
