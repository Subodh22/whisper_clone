# Bol

System-wide dictation powered by Whisper AI — free, offline, private. Hold
`Ctrl+Shift+Space` anywhere to record; on release the audio is transcribed
locally and typed into the focused app.

## How it works

1. **Hold** Ctrl+Shift+Space → overlay appears, mic starts recording
2. **Release** → overlay switches to "thinking" animation (3 pulsing dots) while Whisper runs
3. **Done** → overlay hides, transcribed text is typed into the focused window
4. **Escape** (while recording) → cancel, nothing is typed

## Modules (`src/`)

- `main.rs` — winit event loop, `AppEvent` state machine, tray icon, hotkey polling thread
- `hotkey.rs` — polls hardware key state every 30ms. `CGEventSourceKeyState` on macOS,
  `GetAsyncKeyState` on Windows. No Accessibility permission needed for detection.
- `audio.rs` — cpal microphone capture → 16 kHz mono f32 buffer
- `transcriber.rs` — whisper-rs inference. State is reused across calls (no re-allocation).
  GPU disabled (`use_gpu(false)`) — Metal from a background thread crashes on macOS.
- `model.rs` — downloads/locates the Whisper GGML model from Hugging Face
- `typer.rs` — enigo types transcribed text into the focused app (needs Accessibility permission)
- `feedback.rs` — start/stop/error audio cues (afplay on macOS, SystemSounds on Windows)
- `overlay.rs` — always-on-top recording indicator. Dark pill rendered with softbuffer
  (raw ARGB pixels). Recording: dotted waveform. Transcribing: cascading thinking dots.
- `settings.rs` — loads/saves `~/.bol/config.toml`

## Settings (`~/.bol/config.toml`)

Created automatically on first run with defaults:

```toml
language = "en"        # Whisper language code: "en", "es", "fr", "de", "zh", ...
                       # Use "auto" for automatic detection (slower)
max_recording_secs = 60  # Auto-stop recording after this many seconds
```

## Build & run

```sh
cargo build --release                    # small.en model (~466 MB)
cargo build --release --features medium  # medium.en model (~1.5 GB, more accurate)
cargo run --release
cargo check                              # fast type-check, no binary
```

## Permissions (macOS)

- **Microphone** — granted via system prompt on first recording attempt
- **Accessibility** — required for enigo to type text into other apps.
  Grant in System Settings → Privacy & Security → Accessibility

## Conventions

- Conventional commits
- No GPU usage from background threads (use_gpu = false)
