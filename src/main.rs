mod audio;
mod feedback;
mod hotkey;
mod model;
mod overlay;
mod settings;
mod transcriber;
mod typer;

use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem},
    TrayIconBuilder,
};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalPosition, LogicalSize};
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId, WindowLevel};

const OVERLAY_W: f64 = 320.0;
const OVERLAY_H: f64 = 84.0;

fn peak_from_buf(buf: &Mutex<Vec<f32>>) -> f32 {
    let b = buf.lock().unwrap();
    if b.is_empty() {
        return 0.0;
    }
    let recent = &b[b.len().saturating_sub(2048)..];
    recent.iter().map(|s| s.abs()).fold(0.0f32, f32::max)
}

/// Generate a 32×32 RGBA icon: dark gray ring with a red center dot.
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

#[derive(Debug, Clone)]
enum AppEvent {
    StartRecording,
    StopRecording,
    CancelRecording,
    AudioLevel(f32),
    /// Animation tick during transcription (drives the thinking dots).
    Tick,
    /// Whisper finished; hide the overlay.
    TranscriptionDone,
}

struct App {
    overlay: Option<overlay::Overlay>,
    tray: Option<tray_icon::TrayIcon>,
    quit_id: Option<tray_icon::menu::MenuId>,
    recorder: Arc<Mutex<audio::Recorder>>,
    transcriber: Arc<transcriber::Transcriber>,
    proxy: winit::event_loop::EventLoopProxy<AppEvent>,
    level_stop: Option<Arc<AtomicBool>>,
    tick_stop: Option<Arc<AtomicBool>>,
    max_recording_secs: u32,
}

impl ApplicationHandler<AppEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // Create overlay window once.
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

        // Create tray icon once (must be on main thread).
        if self.tray.is_none() {
            if let Some(icon) = make_tray_icon() {
                let menu = Menu::new();
                let quit_item = MenuItem::new("Quit Bol", true, None);
                let quit_id = quit_item.id().clone();
                let _ = menu.append(&quit_item);
                match TrayIconBuilder::new()
                    .with_icon(icon)
                    .with_tooltip("Bol — Hold Ctrl+Shift+Space to dictate")
                    .with_menu(Box::new(menu))
                    .build()
                {
                    Ok(tray) => {
                        self.tray = Some(tray);
                        self.quit_id = Some(quit_id);
                    }
                    Err(e) => eprintln!("Tray icon unavailable: {}", e),
                }
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Poll the tray menu channel — fires when user clicks "Quit Bol".
        if let Ok(event) = MenuEvent::receiver().try_recv() {
            if self.quit_id.as_ref() == Some(&event.id) {
                event_loop.exit();
            }
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
                feedback::play_start();
                {
                    let mut rec = self.recorder.lock().unwrap();
                    if let Err(e) = rec.start() {
                        eprintln!("Recording failed: {}", e);
                        feedback::play_error();
                        return;
                    }
                }
                if let Some(ref mut o) = self.overlay {
                    o.set_recording(true);
                    o.window.set_visible(true);
                }

                let stop = Arc::new(AtomicBool::new(false));
                self.level_stop = Some(stop.clone());
                let audio_buf = self.recorder.lock().unwrap().buffer_arc();
                let proxy = self.proxy.clone();
                let max_secs = self.max_recording_secs as u64;
                let start_time = std::time::Instant::now();

                std::thread::spawn(move || {
                    while !stop.load(Ordering::Relaxed) {
                        let level = peak_from_buf(&audio_buf);
                        let _ = proxy.send_event(AppEvent::AudioLevel(level));
                        // Auto-stop if the user holds the hotkey too long.
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
                if let Some(s) = self.level_stop.take() {
                    s.store(true, Ordering::Relaxed);
                }
                if let Some(s) = self.tick_stop.take() {
                    s.store(true, Ordering::Relaxed);
                }
                let _ = self.recorder.lock().unwrap().stop();
                if let Some(ref mut o) = self.overlay {
                    o.set_recording(false);
                    o.set_transcribing(false);
                    o.window.set_visible(false);
                }
                eprintln!("Recording cancelled.");
            }

            AppEvent::StopRecording => {
                if let Some(s) = self.level_stop.take() {
                    s.store(true, Ordering::Relaxed);
                }
                feedback::play_stop();
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

                // Switch overlay to "thinking" mode — stays visible with animated dots.
                if let Some(ref mut o) = self.overlay {
                    o.set_recording(false);
                    o.set_transcribing(true);
                    o.window.request_redraw();
                }

                // Tick thread keeps the thinking-dots animation moving.
                let tick_stop = Arc::new(AtomicBool::new(false));
                self.tick_stop = Some(tick_stop.clone());
                let tick_proxy = self.proxy.clone();
                std::thread::spawn(move || {
                    while !tick_stop.load(Ordering::Relaxed) {
                        let _ = tick_proxy.send_event(AppEvent::Tick);
                        std::thread::sleep(Duration::from_millis(50));
                    }
                });

                eprintln!("Transcribing {:.1}s...", duration);
                let t = self.transcriber.clone();
                let proxy = self.proxy.clone();
                std::thread::spawn(move || {
                    match t.transcribe(&audio) {
                        Ok(text) => {
                            let text = text.trim().to_string();
                            if text.is_empty() {
                                eprintln!("Nothing detected.");
                            } else {
                                eprintln!("Typing: \"{}\"", text);
                                if let Err(e) = typer::type_text(&text) {
                                    eprintln!("Typing error: {}", e);
                                }
                            }
                        }
                        Err(e) => eprintln!("Transcription error: {}", e),
                    }
                    let _ = proxy.send_event(AppEvent::TranscriptionDone);
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

            AppEvent::TranscriptionDone => {
                if let Some(s) = self.tick_stop.take() {
                    s.store(true, Ordering::Relaxed);
                }
                if let Some(ref mut o) = self.overlay {
                    o.set_transcribing(false);
                    o.window.set_visible(false);
                }
            }
        }
    }
}

fn main() -> Result<()> {
    let cfg = settings::Settings::load();

    eprintln!("Bol — System-wide Whisper Dictation");
    eprintln!("Hold Ctrl+Shift+Space anywhere to dictate.");
    eprintln!(
        "Settings: language={}, max_recording={}s  (edit ~/.bol/config.toml to change)\n",
        cfg.language, cfg.max_recording_secs
    );

    // Select model based on compile-time feature flag.
    #[cfg(feature = "medium")]
    let model_name = "medium.en";
    #[cfg(not(feature = "medium"))]
    let model_name = "small.en";

    let model_path = model::ensure_model(model_name)?;
    let transcriber = Arc::new(transcriber::Transcriber::new(&model_path, &cfg.language)?);
    let recorder = Arc::new(Mutex::new(audio::Recorder::new()?));

    eprintln!("Ready. Hold Ctrl+Shift+Space to start recording.\n");

    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("osascript")
        .args([
            "-e",
            r#"display notification "Hold Ctrl+Shift+Space anywhere to dictate" with title "Bol is ready""#,
        ])
        .spawn();

    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-WindowStyle",
            "Hidden",
            "-Command",
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

    // Hotkey polling thread — CGEventSourceKeyState on macOS, GetAsyncKeyState on Windows.
    {
        let proxy = proxy.clone();
        std::thread::spawn(move || {
            let mut recording = false;
            let mut prev_esc = false;
            loop {
                let hotkey_active =
                    hotkey::ctrl_held() && hotkey::shift_held() && hotkey::space_held();
                let esc = hotkey::esc_held();

                if hotkey_active && !recording {
                    recording = true;
                    let _ = proxy.send_event(AppEvent::StartRecording);
                } else if !hotkey_active && recording {
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
        quit_id: None,
        recorder,
        transcriber,
        proxy,
        level_stop: None,
        tick_stop: None,
        max_recording_secs: cfg.max_recording_secs,
    };

    event_loop.run_app(&mut app)?;
    Ok(())
}
