use anyhow::{anyhow, Result};
use std::path::Path;
use std::sync::Mutex;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState};

/// Wraps whisper-rs to provide a simple transcription interface.
/// The WhisperState (which holds the large GPU compute buffers) is created once
/// and reused across calls — avoids reallocating hundreds of MB per transcription.
pub struct Transcriber {
    _ctx: WhisperContext,
    state: Mutex<WhisperState>,
}

impl Transcriber {
    pub fn new(model_path: &Path) -> Result<Self> {
        if !model_path.exists() {
            return Err(anyhow!("Model file not found: {}", model_path.display()));
        }

        let path_str = model_path
            .to_str()
            .ok_or_else(|| anyhow!("Invalid model path encoding"))?;

        eprintln!("   ⏳ Loading Whisper model...");
        let mut ctx_params = WhisperContextParameters::default();
        ctx_params.use_gpu(true);
        let ctx = WhisperContext::new_with_params(path_str, ctx_params)
            .map_err(|e| anyhow!("Failed to load Whisper model: {:?}", e))?;

        // Allocate GPU compute buffers once here; reuse on every transcription call.
        let state = ctx
            .create_state()
            .map_err(|e| anyhow!("Failed to create Whisper state: {:?}", e))?;

        eprintln!("   ✅ Model loaded successfully\n");

        Ok(Self {
            _ctx: ctx,
            state: Mutex::new(state),
        })
    }

    /// Transcribe 16kHz mono f32 audio data into text.
    pub fn transcribe(&self, audio: &[f32]) -> Result<String> {
        // Trim leading/trailing silence — reduces audio length sent to the model.
        let audio = trim_silence(audio, 0.005);

        let mut state = self.state.lock().unwrap();

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });

        // Use half available CPU cores for encoder parallelism (even with GPU, encoder
        // benefits slightly from CPU threads for pre/post-processing).
        let n_threads = std::thread::available_parallelism()
            .map(|n| (n.get() / 2).max(1) as i32)
            .unwrap_or(4);
        params.set_n_threads(n_threads);

        params.set_language(Some("en"));
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_no_context(true);
        params.set_single_segment(false);
        params.set_suppress_blank(true);

        // By default whisper encodes a full 30-second window (1500 mel frames) regardless
        // of actual audio length. Setting audio_ctx to match the real length makes the
        // encoder proportionally faster: a 3-second clip is ~5× faster to encode.
        // 1 mel frame = 160 samples at 16 kHz.
        let n_audio_frames = ((audio.len() as f32 / 160.0).ceil() as i32).clamp(1, 1500);
        params.set_audio_ctx(n_audio_frames);

        // Dictation sentences rarely exceed ~100 tokens. Capping the decoder here
        // prevents it from running the full 448-token budget on short clips.
        params.set_n_max_text_ctx(224);

        state
            .full(params, audio)
            .map_err(|e| anyhow!("Whisper inference failed: {:?}", e))?;

        let n_segments = state.full_n_segments();
        let mut text = String::new();
        for i in 0..n_segments {
            let segment = state
                .get_segment(i)
                .ok_or_else(|| anyhow!("Failed to get segment {}", i))?;
            text.push_str(
                segment
                    .to_str()
                    .map_err(|e| anyhow!("Failed to get segment text {}: {:?}", i, e))?,
            );
        }

        Ok(text)
    }
}

/// Strip leading and trailing silence below `threshold` amplitude.
/// Leaves the audio untouched if no sample exceeds the threshold.
fn trim_silence(audio: &[f32], threshold: f32) -> &[f32] {
    let start = audio
        .iter()
        .position(|&s| s.abs() > threshold)
        .unwrap_or(0);
    let end = audio
        .iter()
        .rposition(|&s| s.abs() > threshold)
        .map(|i| i + 1)
        .unwrap_or(audio.len());
    if start < end {
        &audio[start..end]
    } else {
        audio
    }
}
