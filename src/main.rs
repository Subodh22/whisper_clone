mod audio;
mod feedback;
mod hotkey;
mod model;
mod overlay;
mod transcriber;
mod typer;

use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalPosition, LogicalSize};
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId, WindowLevel};

fn peak_from_buf(buf: &Mutex<Vec<f32>>) -> f32 {
    let b = buf.lock().unwrap();
    if b.is_empty() {
        return 0.0;
    }
    let recent = &b[b.len().saturating_sub(2048)..];
    recent.iter().map(|s| s.abs()).fold(0.0f32, f32::max)
}

#[derive(Debug, Clone)]
enum AppEvent {
    StartRecording,
    StopRecording,
    CancelRecording,
    AudioLevel(f32),
}

struct App {
    overlay: Option<overlay::Overlay>,
    recorder: Arc<Mutex<audio::Recorder>>,
    transcriber: Arc<transcriber::Transcriber>,
    proxy: winit::event_loop::EventLoopProxy<AppEvent>,
    level_stop: Option<Arc<AtomicBool>>,
}

impl ApplicationHandler<AppEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.overlay.is_some() {
            return;
        }

        let attrs = Window::default_attributes()
            .with_inner_size(LogicalSize::new(500.0f64, 72.0))
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
                (sw - 500.0) / 2.0,
                sh - 72.0 - 120.0,
            ));
        }

        match overlay::Overlay::new(window) {
            Ok(o) => self.overlay = Some(o),
            Err(e) => eprintln!("Overlay init failed: {}", e),
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
                std::thread::spawn(move || {
                    while !stop.load(Ordering::Relaxed) {
                        let level = peak_from_buf(&audio_buf);
                        let _ = proxy.send_event(AppEvent::AudioLevel(level));
                        std::thread::sleep(Duration::from_millis(50));
                    }
                });
            }

            AppEvent::CancelRecording => {
                if let Some(s) = self.level_stop.take() {
                    s.store(true, Ordering::Relaxed);
                }
                let _ = self.recorder.lock().unwrap().stop();
                if let Some(ref mut o) = self.overlay {
                    o.set_recording(false);
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
                if let Some(ref mut o) = self.overlay {
                    o.set_recording(false);
                    o.window.set_visible(false);
                }
                let duration = audio.len() as f32 / 16_000.0;
                if duration < 0.3 {
                    eprintln!("Too short ({:.1}s), skipping.", duration);
                    return;
                }
                eprintln!("Transcribing {:.1}s...", duration);
                let t = self.transcriber.clone();
                std::thread::spawn(move || match t.transcribe(&audio) {
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
                });
            }

            AppEvent::AudioLevel(level) => {
                if let Some(ref mut o) = self.overlay {
                    o.push_level(level);
                    o.window.request_redraw();
                }
            }
        }
    }
}

fn main() -> Result<()> {
    eprintln!("VoxType — System-wide Whisper Dictation");
    eprintln!("Hold Ctrl+Shift+Space anywhere to dictate.\n");

    let model_path = model::ensure_model("small.en")?;
    let transcriber = Arc::new(transcriber::Transcriber::new(&model_path)?);
    let recorder = Arc::new(Mutex::new(audio::Recorder::new()?));

    eprintln!("Ready. Hold Ctrl+Shift+Space to start recording.\n");

    // Show a notification so the user knows the app is running
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("osascript")
        .args(["-e", r#"display notification "Hold Ctrl+Shift+Space anywhere to dictate" with title "VoxType is ready""#])
        .spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("powershell")
        .args([
            "-NoProfile", "-WindowStyle", "Hidden", "-Command",
            concat!(
                r#"$xml=[xml]'<toast><visual><binding template="ToastGeneric">"#,
                r#"<text>VoxType is ready</text>"#,
                r#"<text>Hold Ctrl+Shift+Space to dictate</text>"#,
                r#"</binding></visual></toast>';"#,
                r#"[Windows.UI.Notifications.ToastNotificationManager,"#,
                r#"Windows.UI.Notifications,ContentType=WindowsRuntime]"#,
                r#"::CreateToastNotifier('VoxType').Show("#,
                r#"[Windows.UI.Notifications.ToastNotification,"#,
                r#"Windows.UI.Notifications,ContentType=WindowsRuntime]::new($xml))"#,
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

    // Hotkey polling thread — reads hardware key state via CGEventSourceKeyState.
    // Replaces rdev, which crashes on macOS 15 due to a TSM thread assertion.
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
        recorder,
        transcriber,
        proxy,
        level_stop: None,
    };

    event_loop.run_app(&mut app)?;
    Ok(())
}
