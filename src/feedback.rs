use std::process::Command;

/// Play a short high-pitched beep to signal recording has started.
/// Uses macOS system sounds via `afplay` for zero dependencies.
pub fn play_start() {
    // Tink = subtle, high-pitched tap
    let _ = Command::new("afplay")
        .arg("/System/Library/Sounds/Tink.aiff")
        .arg("-v")
        .arg("0.5")
        .spawn();
}

/// Play a short lower-pitched beep to signal recording has stopped.
pub fn play_stop() {
    // Pop = slightly lower tone
    let _ = Command::new("afplay")
        .arg("/System/Library/Sounds/Pop.aiff")
        .arg("-v")
        .arg("0.5")
        .spawn();
}

/// Play an error sound.
pub fn play_error() {
    let _ = Command::new("afplay")
        .arg("/System/Library/Sounds/Basso.aiff")
        .arg("-v")
        .arg("0.3")
        .spawn();
}
