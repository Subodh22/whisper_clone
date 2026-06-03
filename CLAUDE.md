# Bol — System-wide Whisper Dictation

Offline, system-wide speech-to-text for Windows (and macOS). Hold a hotkey, speak, release — the transcribed text is typed into whatever window has focus. Runs entirely on-device using whisper.cpp with optional CUDA acceleration.

## Build & Run

```powershell
# Debug build (fast compile, slow inference)
cargo build

# Release build — small.en model (default)
cargo build --release

# Release build — medium.en model (more accurate, ~1.5 GB)
cargo build --release --features medium

# Run directly
cargo run --release
```

The binary is `target/release/bol.exe`. On first run it downloads the model to `%APPDATA%\bol\` (~150 MB for small.en).

## CI

GitHub Actions builds `bol.exe` on `push` to `main` via `.github/workflows/build-windows.yml`. Artifact is uploaded as `bol-windows-x64`. No tests — the build passing is the gate.

## Architecture

```
src/
  main.rs        — winit event loop, App state, tray icon, hotkey thread
  overlay.rs     — floating recording overlay window (softbuffer pixel rendering)
  status.rs      — always-visible status widget (model + device info)
  audio.rs       — cpal microphone capture, 16 kHz resampling
  transcriber.rs — whisper-rs wrapper, WhisperState reused across calls
  hotkey.rs      — OS key-state polling (no event tap, no special permissions)
  model.rs       — model download + path resolution
  typer.rs       — types transcribed text via enigo
  feedback.rs    — audio feedback (start/stop sounds)
```

### Event flow

```
hotkey thread → proxy.send_event(AppEvent::*)
audio level thread → proxy.send_event(AppEvent::AudioLevel)
winit event loop → App::user_event() → drives recorder / overlay / status
```

### AppEvent variants
- `StartRecording` / `StopRecording` / `CancelRecording`
- `AudioLevel(f32)` — peak amplitude, sent every 50 ms while recording
- `SelectDevice(usize)` — switch microphone
- `TrayQuit`

## Hotkey

Currently: hold **Ctrl+Shift** to record, release to transcribe. Esc cancels.

Implemented in `hotkey.rs` via `GetAsyncKeyState` on Windows and `CGEventSourceKeyState` on macOS — no elevated permissions or event taps needed.

## Overlay & Status Windows

- **Overlay** (`src/overlay.rs`): 360×50 px pill, hidden until recording starts. Shows animated waveform bars, "Rec" label, Stop/Cancel controls. Drawn entirely with `softbuffer` pixel writes (no GPU, no UI framework). Uses `fontdue` for text rasterization.
- **Status widget** (`src/status.rs`): 220×36 px always-visible widget in the top-right corner. Shows recording state, model name, and device selector (click arrows to switch mic).

Both windows use `WindowLevel::AlwaysOnTop`, transparent backgrounds, no decorations.

## Key Dependencies

| Crate | Purpose |
|---|---|
| `whisper-rs` | whisper.cpp bindings (CUDA feature enabled) |
| `cpal` | Cross-platform audio capture |
| `winit 0.30` | Window + event loop |
| `softbuffer 0.4` | CPU pixel buffer rendering |
| `fontdue 0.9` | Font rasterization (no GPU) |
| `enigo 0.2` | Simulated keyboard typing |
| `tray-icon 0.20` | System tray icon + menu |
| `windows-sys 0.52` | `GetAsyncKeyState`, etc. |

## Current Branch: `improve-overlay-ui`

Active work is redesigning the overlay to match the WhisperFlow aesthetic:
- Idle: small floating pill with dots
- Hover: expanded card with sparkle / delta / expand icons + "Start recording Ctrl+Space" hint
- Recording: wider pill with red delta button + dot waveform

`src/status.rs` may be absorbed into the overlay once the new UI is complete.

## Notes

- `#[windows_subsystem = "windows"]` suppresses the console window in release; remove it during debugging to see `eprintln!` output.
- `whisper-rs` links whisper.cpp statically via CMake at build time — first build takes several minutes.
- Audio is always captured as 16 kHz mono f32 PCM for Whisper; `Recorder` handles all resampling internally.
- `WhisperState` is allocated once at startup and reused — avoids reallocating GPU buffers (~hundreds of MB) on each dictation.
- The tray icon is drawn with a signed-distance-field renderer directly in `make_tray_icon()` in `main.rs` — no image files.
