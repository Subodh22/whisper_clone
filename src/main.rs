mod audio;
mod feedback;
mod hotkey;
mod launcher;
mod model;
mod overlay;
mod permissions;
mod settings;
mod transcriber;
mod typer;

use anyhow::Result;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tray_icon::{
    menu::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem},
    TrayIconBuilder,
};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalPosition, LogicalSize};
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId, WindowLevel};

const OVERLAY_W: f64 = 320.0;
const OVERLAY_H: f64 = 84.0;
const HISTORY_SLOTS: usize = 3;
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

// ─── Peak level helper ──────────────────────────────────────────────────────

fn peak_from_buf(buf: &Mutex<Vec<f32>>) -> f32 {
    let b = buf.lock().unwrap();
    if b.is_empty() {
        return 0.0;
    }
    let recent = &b[b.len().saturating_sub(2048)..];
    recent.iter().map(|s| s.abs()).fold(0.0f32, f32::max)
}

// ─── Tray icon image ────────────────────────────────────────────────────────

fn make_tray_icon() -> Option<tray_icon::Icon> {
    let size = 32u32;
    let mut data = vec![0u8; (size * size * 4) as usize];
    let cx = size as f32 / 2.0;
    let cy = size as f32 / 2.0;
    let outer_r = size as f32 / 2.0 - 1.0;
    let inner_r = outer_r * 0.52;
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            let idx = ((y * size + x) * 4) as usize;
            if dist <= outer_r {
                data[idx]     = 55;
                data[idx + 1] = 55;
                data[idx + 2] = 58;
                data[idx + 3] = 255;
            }
            if dist <= inner_r {
                data[idx]     = 234;
                data[idx + 1] = 58;
                data[idx + 2] = 46;
                data[idx + 3] = 255;
            }
        }
    }
    tray_icon::Icon::from_rgba(data, size, size).ok()
}

// ─── App events ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum AppEvent {
    StartRecording,
    StopRecording,
    CancelRecording,
    AudioLevel(f32),
    Tick,
    TranscriptionDone(String),
}

// ─── Tray menu state ────────────────────────────────────────────────────────

struct TrayState {
    _tray: tray_icon::TrayIcon,
    quit_id: MenuId,
    sounds_item: CheckMenuItem,
    sounds_id: MenuId,
    overlay_item: CheckMenuItem,
    overlay_id: MenuId,
    login_item: CheckMenuItem,
    login_id: MenuId,
    open_config_id: MenuId,
    hist: [MenuItem; HISTORY_SLOTS],
}

// ─── App struct ─────────────────────────────────────────────────────────────

struct App {
    overlay: Option<overlay::Overlay>,
    tray: Option<TrayState>,
    recorder: Arc<Mutex<audio::Recorder>>,
    transcriber: Arc<transcriber::Transcriber>,
    proxy: winit::event_loop::EventLoopProxy<AppEvent>,
    level_stop: Option<Arc<AtomicBool>>,
    tick_stop: Option<Arc<AtomicBool>>,
    settings: settings::Settings,
    history: VecDeque<String>,
    binary_path: PathBuf,
}

impl App {
    // Rebuild the 3 history slots in the tray menu.
    fn refresh_history_menu(&self) {
        let Some(ref ts) = self.tray else { return };
        for (i, item) in ts.hist.iter().enumerate() {
            if let Some(text) = self.history.get(i) {
                item.set_text(format!("  {}", truncate_menu(text)));
            } else {
                item.set_text("  —");
            }
        }
    }

    fn handle_menu_event(&mut self, id: MenuId, event_loop: &ActiveEventLoop) {
        // Snapshot the IDs we need to compare (ends the tray borrow).
        let ids = match &self.tray {
            Some(ts) => (
                ts.quit_id.clone(),
                ts.sounds_id.clone(),
                ts.overlay_id.clone(),
                ts.login_id.clone(),
                ts.open_config_id.clone(),
            ),
            None => return,
        };

        if id == ids.0 {
            event_loop.exit();
        } else if id == ids.1 {
            let new = !self.settings.feedback_sounds;
            self.settings.feedback_sounds = new;
            if let Some(ref ts) = self.tray { ts.sounds_item.set_checked(new); }
            let _ = self.settings.save();
        } else if id == ids.2 {
            let new = !self.settings.show_overlay;
            self.settings.show_overlay = new;
            if let Some(ref ts) = self.tray { ts.overlay_item.set_checked(new); }
            let _ = self.settings.save();
        } else if id == ids.3 {
            let new = !self.settings.launch_at_login;
            if launcher::set_launch_at_login(new, &self.binary_path).is_ok() {
                self.settings.launch_at_login = new;
                if let Some(ref ts) = self.tray { ts.login_item.set_checked(new); }
                let _ = self.settings.save();
            }
        } else if id == ids.4 {
            open_config_file();
        }
    }
}

// ─── ApplicationHandler ─────────────────────────────────────────────────────

impl ApplicationHandler<AppEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // ── Overlay window ──
        if self.overlay.is_none() {
            let attrs = Window::default_attributes()
                .with_inner_size(LogicalSize::new(OVERLAY_W, OVERLAY_H))
                .with_decorations(false)
                .with_transparent(true)
                .with_window_level(WindowLevel::AlwaysOnTop)
                .with_visible(false)
                .with_resizable(false);

            let window = match event_loop.create_window(attrs) {
                Ok(w) => Arc::new(w),
                Err(e) => {
                    eprintln!("Failed to create overlay window: {}", e);
                    return;
                }
            };
            let _ = window.set_cursor_hittest(false);

            if let Some(monitor) = window.current_monitor() {
                let sf = monitor.scale_factor();
                let size = monitor.size();
                let sw = size.width as f64 / sf;
                let sh = size.height as f64 / sf;
                window.set_outer_position(LogicalPosition::new(
                    (sw - OVERLAY_W) / 2.0,
                    sh - OVERLAY_H - 120.0,
                ));
            }

            match overlay::Overlay::new(window) {
                Ok(o) => self.overlay = Some(o),
                Err(e) => eprintln!("Overlay init failed: {}", e),
            }
        }

        // ── Tray icon + menu (must be on main thread) ──
        if self.tray.is_none() {
            if let Some(icon) = make_tray_icon() {
                let menu = Menu::new();

                // Header (non-interactive)
                let _ = menu.append(&MenuItem::new("Bol", false, None));
                let _ = menu.append(&MenuItem::new(
                    format!("Hold {} to dictate", self.settings.hotkey),
                    false, None,
                ));
                let _ = menu.append(&MenuItem::new("─────────────────────", false, None));

                // Recent transcriptions (3 fixed slots, updated dynamically)
                let _ = menu.append(&MenuItem::new("Recent:", false, None));
                let hist: [MenuItem; HISTORY_SLOTS] = [
                    MenuItem::new("  —", false, None),
                    MenuItem::new("  —", false, None),
                    MenuItem::new("  —", false, None),
                ];
                let hist_handles = [hist[0].clone(), hist[1].clone(), hist[2].clone()];
                for item in &hist { let _ = menu.append(item); }
                let _ = menu.append(&MenuItem::new("─────────────────────", false, None));

                // Toggle settings
                let sounds_item = CheckMenuItem::new(
                    "Sound Feedback", true, self.settings.feedback_sounds, None,
                );
                let sounds_id = sounds_item.id().clone();
                let overlay_item = CheckMenuItem::new(
                    "Show Overlay", true, self.settings.show_overlay, None,
                );
                let overlay_id = overlay_item.id().clone();
                let login_item = CheckMenuItem::new(
                    "Launch at Login", true, self.settings.launch_at_login, None,
                );
                let login_id = login_item.id().clone();
                let _ = menu.append(&sounds_item);
                let _ = menu.append(&overlay_item);
                let _ = menu.append(&login_item);
                let _ = menu.append(&MenuItem::new("─────────────────────", false, None));

                // Utilities
                let open_config = MenuItem::new("Open Config File", true, None);
                let open_config_id = open_config.id().clone();
                let _ = menu.append(&open_config);
                let _ = menu.append(&MenuItem::new("─────────────────────", false, None));

                // Footer
                let _ = menu.append(&MenuItem::new(
                    format!("Bol v{}", APP_VERSION), false, None,
                ));
                let quit_item = MenuItem::new("Quit Bol", true, None);
                let quit_id = quit_item.id().clone();
                let _ = menu.append(&quit_item);

                match TrayIconBuilder::new()
                    .with_icon(icon)
                    .with_tooltip(format!("Bol — {} to dictate", self.settings.hotkey))
                    .with_menu(Box::new(menu))
                    .build()
                {
                    Ok(tray) => {
                        self.tray = Some(TrayState {
                            _tray: tray,
                            quit_id,
                            sounds_item,
                            sounds_id,
                            overlay_item,
                            overlay_id,
                            login_item,
                            login_id,
                            open_config_id,
                            hist: hist_handles,
                        });
                    }
                    Err(e) => eprintln!("Tray icon unavailable: {}", e),
                }
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        while let Ok(ev) = MenuEvent::receiver().try_recv() {
            let id = ev.id;
            self.handle_menu_event(id, event_loop);
        }
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        if let WindowEvent::RedrawRequested = event {
            if let Some(ref mut o) = self.overlay {
                if let Err(e) = o.draw() {
                    eprintln!("Draw error: {}", e);
                }
            }
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: AppEvent) {
        match event {
            AppEvent::StartRecording => {
                if self.settings.feedback_sounds { feedback::play_start(); }

                {
                    let mut rec = self.recorder.lock().unwrap();
                    if let Err(e) = rec.start() {
                        eprintln!("Recording failed: {}", e);
                        if self.settings.feedback_sounds { feedback::play_error(); }
                        return;
                    }
                }

                if self.settings.show_overlay {
                    if let Some(ref mut o) = self.overlay {
                        o.set_recording(true);
                        o.window.set_visible(true);
                    }
                }

                let stop = Arc::new(AtomicBool::new(false));
                self.level_stop = Some(stop.clone());
                let audio_buf = self.recorder.lock().unwrap().buffer_arc();
                let proxy = self.proxy.clone();
                let max_secs = self.settings.max_recording_secs as u64;
                let start_time = std::time::Instant::now();

                std::thread::spawn(move || {
                    while !stop.load(Ordering::Relaxed) {
                        let level = peak_from_buf(&audio_buf);
                        let _ = proxy.send_event(AppEvent::AudioLevel(level));
                        if start_time.elapsed().as_secs() >= max_secs {
                            eprintln!("Max recording time ({max_secs}s) reached, stopping.");
                            let _ = proxy.send_event(AppEvent::StopRecording);
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(50));
                    }
                });
            }

            AppEvent::CancelRecording => {
                if let Some(s) = self.level_stop.take() { s.store(true, Ordering::Relaxed); }
                if let Some(s) = self.tick_stop.take() { s.store(true, Ordering::Relaxed); }
                let _ = self.recorder.lock().unwrap().stop();
                if let Some(ref mut o) = self.overlay {
                    o.set_recording(false);
                    o.set_transcribing(false);
                    o.window.set_visible(false);
                }
                eprintln!("Recording cancelled.");
            }

            AppEvent::StopRecording => {
                if let Some(s) = self.level_stop.take() { s.store(true, Ordering::Relaxed); }
                if self.settings.feedback_sounds { feedback::play_stop(); }
                let audio = self.recorder.lock().unwrap().stop();

                let duration = audio.len() as f32 / 16_000.0;
                if duration < 0.3 {
                    eprintln!("Too short ({:.1}s), skipping.", duration);
                    if let Some(ref mut o) = self.overlay {
                        o.set_recording(false);
                        o.window.set_visible(false);
                    }
                    return;
                }

                // Switch overlay to thinking-dots mode.
                if let Some(ref mut o) = self.overlay {
                    o.set_recording(false);
                    o.set_transcribing(true);
                    o.window.request_redraw();
                }

                // Tick thread drives the thinking-dots animation.
                let tick_stop = Arc::new(AtomicBool::new(false));
                self.tick_stop = Some(tick_stop.clone());
                let tick_proxy = self.proxy.clone();
                std::thread::spawn(move || {
                    while !tick_stop.load(Ordering::Relaxed) {
                        let _ = tick_proxy.send_event(AppEvent::Tick);
                        std::thread::sleep(Duration::from_millis(50));
                    }
                });

                eprintln!("Transcribing {:.1}s of audio...", duration);
                let t = self.transcriber.clone();
                let proxy = self.proxy.clone();
                std::thread::spawn(move || {
                    let text = match t.transcribe(&audio) {
                        Ok(raw) => {
                            let text = raw.trim().to_string();
                            if text.is_empty() {
                                eprintln!("Nothing detected.");
                            } else {
                                eprintln!("Typing: {:?}", text);
                                if let Err(e) = typer::type_text(&text) {
                                    eprintln!("Typing error: {}", e);
                                }
                            }
                            text
                        }
                        Err(e) => {
                            eprintln!("Transcription error: {}", e);
                            String::new()
                        }
                    };
                    let _ = proxy.send_event(AppEvent::TranscriptionDone(text));
                });
            }

            AppEvent::AudioLevel(level) => {
                if let Some(ref mut o) = self.overlay {
                    o.push_level(level);
                    o.window.request_redraw();
                }
            }

            AppEvent::Tick => {
                if let Some(ref mut o) = self.overlay {
                    o.tick();
                    o.window.request_redraw();
                }
            }

            AppEvent::TranscriptionDone(text) => {
                if let Some(s) = self.tick_stop.take() { s.store(true, Ordering::Relaxed); }
                if let Some(ref mut o) = self.overlay {
                    o.set_transcribing(false);
                    o.window.set_visible(false);
                }
                // Update in-memory history and tray menu slots.
                if !text.is_empty() {
                    self.history.push_front(text.clone());
                    if self.history.len() > HISTORY_SLOTS {
                        self.history.pop_back();
                    }
                    self.refresh_history_menu();
                    append_history_file(&text);
                }
            }
        }
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn truncate_menu(s: &str) -> String {
    const MAX: usize = 45;
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= MAX {
        s.to_string()
    } else {
        format!("{}…", chars[..MAX - 1].iter().collect::<String>())
    }
}

fn open_config_file() {
    let path = match dirs::home_dir() {
        Some(h) => h.join(".bol").join("config.toml"),
        None => return,
    };
    // Ensure file exists before opening.
    if !path.exists() {
        let _ = settings::Settings::default().save();
    }
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(&path).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("notepad").arg(&path).spawn();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let _ = std::process::Command::new("xdg-open").arg(&path).spawn();
}

fn append_history_file(text: &str) {
    let path = match dirs::home_dir() {
        Some(h) => h.join(".bol").join("history.txt"),
        None => return,
    };
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let entry = format!("[{}] {}\n", secs, text.trim());
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map(|mut f| { use std::io::Write; f.write_all(entry.as_bytes()) });
}

// ─── main ────────────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    let mut cfg = settings::Settings::load();

    eprintln!("Bol v{} — System-wide Whisper Dictation", APP_VERSION);
    eprintln!("Hold {} anywhere to dictate.", cfg.hotkey);
    eprintln!(
        "Settings: language={}, max={}s, sounds={}, overlay={}, hotkey={}",
        cfg.language, cfg.max_recording_secs, cfg.feedback_sounds,
        cfg.show_overlay, cfg.hotkey
    );
    eprintln!("Edit ~/.bol/config.toml to change settings.\n");

    // Check Accessibility permission (needed for typing text into other apps).
    permissions::check_and_warn_accessibility();

    // Select model: small.en by default, medium.en with --features medium.
    #[cfg(feature = "medium")]
    let model_name = "medium.en";
    #[cfg(not(feature = "medium"))]
    let model_name = "small.en";

    let model_path = model::ensure_model(model_name)?;
    let transcriber = Arc::new(transcriber::Transcriber::new(&model_path, &cfg.language)?);
    let recorder = Arc::new(Mutex::new(audio::Recorder::new()?));
    let binary_path = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("bol"));
    let hotkey_def = hotkey::parse_hotkey(&cfg.hotkey);
    // Sync launch_at_login with actual OS state in case plist was manually added/removed.
    cfg.launch_at_login = launcher::is_launch_at_login();

    eprintln!("Ready. Hold {} to start recording.\n", cfg.hotkey);

    // Platform startup notification
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("osascript")
        .args([
            "-e",
            &format!(
                r#"display notification "Hold {} anywhere to dictate" with title "Bol is ready""#,
                cfg.hotkey
            ),
        ])
        .spawn();

    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("powershell")
        .args([
            "-NoProfile", "-WindowStyle", "Hidden", "-Command",
            concat!(
                "Add-Type -AssemblyName System.Windows.Forms; ",
                "$n = New-Object System.Windows.Forms.NotifyIcon; ",
                "$n.Icon = [System.Drawing.SystemIcons]::Application; ",
                "$n.Visible = $true; ",
                "$n.ShowBalloonTip(4000, 'Bol is ready', 'Hold Ctrl+Shift+Space to dictate', 'Info'); ",
                "Start-Sleep -s 5; $n.Dispose()",
            ),
        ])
        .spawn();

    // Build event loop (background-only on macOS — no Dock icon).
    #[cfg(target_os = "macos")]
    let event_loop = {
        use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
        EventLoop::<AppEvent>::with_user_event()
            .with_activation_policy(ActivationPolicy::Accessory)
            .build()?
    };
    #[cfg(not(target_os = "macos"))]
    let event_loop = EventLoop::<AppEvent>::with_user_event().build()?;

    let proxy = event_loop.create_proxy();

    // Hotkey polling thread — uses platform HID state, no permissions needed.
    {
        let proxy = proxy.clone();
        let hotkey_def = hotkey_def.clone();
        std::thread::spawn(move || {
            let mut recording = false;
            let mut prev_esc = false;
            loop {
                let active = hotkey::hotkey_active(&hotkey_def);
                let esc = hotkey::esc_held();

                if active && !recording {
                    recording = true;
                    let _ = proxy.send_event(AppEvent::StartRecording);
                } else if !active && recording {
                    recording = false;
                    let _ = proxy.send_event(AppEvent::StopRecording);
                }

                if esc && !prev_esc && recording {
                    recording = false;
                    let _ = proxy.send_event(AppEvent::CancelRecording);
                }
                prev_esc = esc;

                std::thread::sleep(Duration::from_millis(30));
            }
        });
    }

    let mut app = App {
        overlay: None,
        tray: None,
        recorder,
        transcriber,
        proxy,
        level_stop: None,
        tick_stop: None,
        settings: cfg,
        history: VecDeque::new(),
        binary_path,
    };

    event_loop.run_app(&mut app)?;
    Ok(())
}
