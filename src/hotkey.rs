// Polls hardware key state via CGEventSourceKeyState.
// Unlike CGEventTap (used by rdev), this reads directly from the HID layer:
// no TSM calls, no Accessibility permission required, safe from any thread.
// macOS 15 added a dispatch_queue_assert to TSMGetInputSourceProperty that
// kills rdev's background-thread callback — this approach sidesteps that entirely.

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventSourceKeyState(state_id: i32, virtual_key: u16) -> bool;
}

// kCGEventSourceStateHIDSystemState = 1 (raw hardware, no permission needed)
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
