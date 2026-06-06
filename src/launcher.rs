use anyhow::Result;
use std::path::Path;

// ───────────────────────────── macOS ─────────────────────────────
#[cfg(target_os = "macos")]
fn plist_path() -> Option<std::path::PathBuf> {
    dirs::home_dir().map(|h| {
        h.join("Library")
            .join("LaunchAgents")
            .join("com.bol.dictation.plist")
    })
}

#[cfg(target_os = "macos")]
pub fn is_launch_at_login() -> bool {
    plist_path().map(|p| p.exists()).unwrap_or(false)
}

#[cfg(target_os = "macos")]
pub fn set_launch_at_login(enabled: bool, binary_path: &Path) -> Result<()> {
    let path = plist_path()
        .ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?;

    if enabled {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let binary = binary_path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid binary path"))?;
        let plist = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.bol.dictation</string>
    <key>ProgramArguments</key>
    <array>
        <string>{binary}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <false/>
    <key>StandardOutPath</key>
    <string>{home}/.bol/bol.log</string>
    <key>StandardErrorPath</key>
    <string>{home}/.bol/bol.log</string>
</dict>
</plist>"#,
            binary = binary,
            home = dirs::home_dir()
                .map(|h| h.to_string_lossy().into_owned())
                .unwrap_or_default()
        );
        std::fs::write(&path, plist)?;
        eprintln!("Launch at login enabled (takes effect on next login).");
    } else if path.exists() {
        std::fs::remove_file(&path)?;
        eprintln!("Launch at login disabled.");
    }
    Ok(())
}

// ───────────────────────────── Windows ─────────────────────────────
#[cfg(target_os = "windows")]
pub fn is_launch_at_login() -> bool {
    std::process::Command::new("reg")
        .args([
            "query",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
            "/v",
            "Bol",
        ])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(target_os = "windows")]
pub fn set_launch_at_login(enabled: bool, binary_path: &Path) -> Result<()> {
    if enabled {
        let binary = binary_path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid binary path"))?;
        let _ = std::process::Command::new("reg")
            .args([
                "add",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                "/v",
                "Bol",
                "/t",
                "REG_SZ",
                "/d",
                binary,
                "/f",
            ])
            .output();
    } else {
        let _ = std::process::Command::new("reg")
            .args([
                "delete",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                "/v",
                "Bol",
                "/f",
            ])
            .output();
    }
    Ok(())
}

// ───────────────────────────── Other platforms ─────────────────────────────
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn is_launch_at_login() -> bool {
    false
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn set_launch_at_login(_enabled: bool, _binary_path: &Path) -> Result<()> {
    Ok(())
}
