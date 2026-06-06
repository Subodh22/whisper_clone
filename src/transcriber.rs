use anyhow::{anyhow, Result};
use std::path::Path;
use std::sync::Mutex;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState};

pub struct Transcriber {
    _ctx: WhisperContext,
    // WhisperState holds large compute buffers — allocate once and reuse.
    state: Mutex<WhisperState>,
    language: String,
}

impl Transcriber {
    pub fn new(model_path: &Path, language: &str) -> Result<Self> {
        if !model_path.exists() {
            return Err(anyhow!("Model file not found: {}", model_path.display()));
        }

        let path_str = model_path
            .to_str()
            .ok_or_else(|| anyhow!("Invalid model path encoding"))?;

        eprintln!("   ⏳ Loading Whisper model...");
        let mut ctx_params = WhisperContextParameters::default();
        // GPU (Metal) must NOT be used from a background thread on macOS — it causes
        // a crash deep in Metal's command queue. CPU inference is safe from any thread.
        ctx_params.use_gpu(false);
        let ctx = WhisperContext::new_with_params(path_str, ctx_params)
            .map_err(|e| anyhow!("Failed to load Whisper model: {:?}", e))?;

        let state = ctx
            .create_state()
            .map_err(|e| anyhow!("Failed to create Whisper state: {:?}", e))?;

        eprintln!("   ✅ Model loaded (language: {})\n", language);

        Ok(Self {
            _ctx: ctx,
            state: Mutex::new(state),
            language: language.to_string(),
        })
    }

    pub fn transcribe(&self, audio: &[f32]) -> Result<String> {
        let audio = trim_silence(audio, 0.005);

        let mut state = self.state.lock().unwrap();

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });

        // Half available cores is a good sweet spot for the Whisper encoder.
        let n_threads = std::thread::available_parallelism()
            .map(|n| (n.get() / 2).max(1) as i32)
            .unwrap_or(4);
        params.set_n_threads(n_threads);

        // "auto" lets Whisper detect language per utterance (slower but multilingual).
        let lang_opt = if self.language == "auto" { None } else { Some(self.language.as_str()) };
        params.set_language(lang_opt);

        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_no_context(true);
        params.set_single_segment(false);
        params.set_suppress_blank(true);

        // Whisper always encodes a full 30-second window (1500 mel frames) unless told
        // otherwise. Shrinking audio_ctx to match actual audio length makes short
        // clips proportionally faster (a 3s clip is ~5x faster than a 30s one).
        let n_audio_frames = ((audio.len() as f32 / 160.0).ceil() as i32).clamp(1, 1500);
        params.set_audio_ctx(n_audio_frames);

        // Dictation sentences rarely exceed ~100 tokens; cap the decoder to avoid
        // spending the full 448-token budget on short clips.
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

/// Strip leading/trailing silence below `threshold` to reduce audio sent to Whisper.
fn trim_silence(audio: &[f32], threshold: f32) -> &[f32] {
    let start = audio.iter().position(|&s| s.abs() > threshold).unwrap_or(0);
    let end = audio
        .iter()
        .rposition(|&s| s.abs() > threshold)
        .map(|i| i + 1)
        .unwrap_or(audio.len());
    if start < end { &audio[start..end] } else { audio }
}
