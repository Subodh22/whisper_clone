use std::process::Command;

#[cfg(target_os = "macos")]
pub fn play_start() {
    let _ = Command::new("afplay")
        .args(["/System/Library/Sounds/Tink.aiff", "-v", "0.5"])
        .spawn();
}

#[cfg(target_os = "macos")]
pub fn play_stop() {
    let _ = Command::new("afplay")
        .args(["/System/Library/Sounds/Pop.aiff", "-v", "0.5"])
        .spawn();
}

#[cfg(target_os = "macos")]
pub fn play_error() {
    let _ = Command::new("afplay")
        .args(["/System/Library/Sounds/Basso.aiff", "-v", "0.3"])
        .spawn();
}

// Windows: use PowerShell to play built-in SystemSounds (async, non-blocking)
#[cfg(target_os = "windows")]
pub fn play_start() {
    let _ = Command::new("powershell")
        .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command",
               "[System.Media.SystemSounds]::Asterisk.Play()"])
        .spawn();
}

#[cfg(target_os = "windows")]
pub fn play_stop() {
    let _ = Command::new("powershell")
        .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command",
               "[System.Media.SystemSounds]::Beep.Play()"])
        .spawn();
}

#[cfg(target_os = "windows")]
pub fn play_error() {
    let _ = Command::new("powershell")
        .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command",
               "[System.Media.SystemSounds]::Hand.Play()"])
        .spawn();
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn play_start() {}
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn play_stop() {}
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn play_error() {}
