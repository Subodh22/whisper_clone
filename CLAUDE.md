# Bol

System-wide dictation powered by Whisper AI — free, offline, private. Hold
`Ctrl+Shift+Space` anywhere to record; on release the audio is transcribed
locally and typed into the focused app.

## Architecture

A single Rust binary built on a `winit` event loop. Modules in `src/`:

- `main.rs` — app entry point, the `winit` event loop, and the `AppEvent`
  state machine (start / stop / cancel recording, audio-level ticks). Spawns a
  background hotkey-polling thread and an audio-level thread.
- `hotkey.rs` — polls hardware key state (Ctrl/Shift/Space/Esc). Uses
  `CGEventSourceKeyState` on macOS and `windows-sys` on Windows.
- `audio.rs` — `cpal` microphone capture into a shared 16 kHz buffer.
- `transcriber.rs` — `whisper-rs` inference (CUDA-enabled).
- `model.rs` — downloads / locates the Whisper model (`small.en` by default,
  `medium.en` with `--features medium`).
- `typer.rs` — types transcribed text into the focused app via `enigo`.
- `feedback.rs` — start/stop/error audio cues.
- `overlay.rs` — the always-on-top recording overlay. CPU-rendered with
  `softbuffer` (raw ARGB pixel buffer); fonts via `fontdue`. This is the file to
  edit for UI/visual changes.

## UI / overlay

The overlay is a transparent, click-through, always-on-top window shown only
while recording. It is drawn as a **dark pill**: a red rounded-square record
button (triangle logo) on the left and a dotted waveform that reacts to mic
level on the right. All drawing is manual pixel work in `overlay.rs::draw_frame`
— there is no GPU/HTML layer. Window size and on-screen position are set in
`main.rs::resumed` (`OVERLAY_W` / `OVERLAY_H`).

## Build & run

```sh
cargo build --release            # small.en model
cargo build --release --features medium
cargo run --release
cargo check                      # fast type-check
```

`whisper-rs` is built with the `cuda` feature, so a CUDA toolkit is required to
compile/link on this machine.

## Conventions

- Conventional commits.
- Always run tests before pushing.
