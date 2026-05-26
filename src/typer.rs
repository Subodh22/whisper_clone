use anyhow::Result;
use enigo::{Enigo, Keyboard, Settings};
use std::thread;
use std::time::Duration;

/// Type the given text into the currently focused input field
/// using simulated keyboard input.
pub fn type_text(text: &str) -> Result<()> {
    if text.is_empty() {
        return Ok(());
    }

    // Small delay to allow key release events to propagate
    // before we start typing
    thread::sleep(Duration::from_millis(100));

    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| anyhow::anyhow!("Failed to initialize keyboard simulator: {:?}", e))?;

    // Type the text — enigo handles Unicode and special characters
    enigo
        .text(text)
        .map_err(|e| anyhow::anyhow!("Failed to type text: {:?}", e))?;

    Ok(())
}
