/// Check Accessibility permission and print a helpful guide if missing.
pub fn check_and_warn_accessibility() {
    if !is_accessibility_trusted() {
        eprintln!();
        eprintln!("┌─────────────────────────────────────────────────────┐");
        eprintln!("│  ⚠  Accessibility permission required               │");
        eprintln!("│                                                      │");
        eprintln!("│  Bol needs Accessibility to type into other apps.   │");
        eprintln!("│                                                      │");
        eprintln!("│  1. Open System Settings (just opened for you)      │");
        eprintln!("│  2. Privacy & Security → Accessibility              │");
        eprintln!("│  3. Enable Bol (or Terminal if running via cargo)   │");
        eprintln!("│  4. Restart Bol                                     │");
        eprintln!("└─────────────────────────────────────────────────────┘");
        eprintln!();
        open_accessibility_settings();
    }
}

// ───────────────────────────── macOS ─────────────────────────────
#[cfg(target_os = "macos")]
#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> bool;
}

#[cfg(target_os = "macos")]
pub fn is_accessibility_trusted() -> bool {
    unsafe { AXIsProcessTrusted() }
}

#[cfg(target_os = "macos")]
pub fn open_accessibility_settings() {
    let _ = std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
        .spawn();
}

// ───────────────────────────── Other platforms ─────────────────────────────
#[cfg(not(target_os = "macos"))]
pub fn is_accessibility_trusted() -> bool {
    true
}

#[cfg(not(target_os = "macos"))]
pub fn open_accessibility_settings() {}
