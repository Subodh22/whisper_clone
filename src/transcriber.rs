use anyhow::{anyhow, Result};
use std::path::Path;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

/// Wraps whisper-rs to provide a simple transcription interface.
pub struct Transcriber {
    ctx: WhisperContext,
}

impl Transcriber {
    /// Load a Whisper GGML model from the given path.
    pub fn new(model_path: &Path) -> Result<Self> {
        if !model_path.exists() {
            return Err(anyhow!("Model file not found: {}", model_path.display()));
        }

        let path_str = model_path
            .to_str()
            .ok_or_else(|| anyhow!("Invalid model path encoding"))?;

        eprintln!("   ⏳ Loading Whisper model...");
        let mut ctx_params = WhisperContextParameters::default();
        ctx_params.use_gpu(false);
        let ctx = WhisperContext::new_with_params(path_str, ctx_params)
            .map_err(|e| anyhow!("Failed to load Whisper model: {:?}", e))?;
        eprintln!("   ✅ Model loaded successfully\n");

        Ok(Self { ctx })
    }

    /// Transcribe 16kHz mono f32 audio data into text.
    pub fn transcribe(&self, audio: &[f32]) -> Result<String> {
        let mut state = self
            .ctx
            .create_state()
            .map_err(|e| anyhow!("Failed to create Whisper state: {:?}", e))?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });

        // Use half the available CPU cores — good sweet spot for whisper encoder parallelism
        let n_threads = std::thread::available_parallelism()
            .map(|n| (n.get() / 2).max(1) as i32)
            .unwrap_or(4);
        params.set_n_threads(n_threads);

        // Configure for dictation: fast, no debug output
        params.set_language(Some("en"));
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_no_context(true);
        params.set_single_segment(false);

        // Suppress non-speech tokens for cleaner dictation output
        params.set_suppress_blank(true);

        // Run inference
        state
            .full(params, audio)
            .map_err(|e| anyhow!("Whisper inference failed: {:?}", e))?;

        // Collect all segments into a single string
        let n_segments = state
            .full_n_segments()
            .map_err(|e| anyhow!("Failed to get segment count: {:?}", e))?;

        let mut text = String::new();
        for i in 0..n_segments {
            let segment = state
                .full_get_segment_text(i)
                .map_err(|e| anyhow!("Failed to get segment {}: {:?}", i, e))?;
            text.push_str(&segment);
        }

        Ok(text)
    }
}
