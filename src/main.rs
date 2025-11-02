use std::error::Error;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tokio::sync::mpsc;
use dotenv::dotenv;

mod audio;
mod transcribers;

use audio::capture_audio_from_mic;
use transcribers::{TranscriberConfig, create_transcriber};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv().ok();

    // Load API key from environment
    let api_key = std::env::var("DEEPGRAM_API_KEY")
        .unwrap_or_else(|_| "YOUR_DEEPGRAM_API_KEY".to_string());

    // Create transcriber based on configuration
    let config = TranscriberConfig::Deepgram { api_key };
    let mut transcriber = create_transcriber(config)?;

    let (tx, rx) = mpsc::unbounded_channel();
    let should_stop = Arc::new(AtomicBool::new(false));
    let should_stop_clone = Arc::clone(&should_stop);

    // Setup Ctrl+C handler
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        should_stop_clone.store(true, Ordering::SeqCst);
    });

    let audio_thread = std::thread::spawn(move || {
        if let Err(err) = capture_audio_from_mic(tx, should_stop) {
            eprintln!("Failed to capture audio: {err}");
        }
    });

    // Initialize transcriber with sample rate and channels
    transcriber.initialize(48000, 1).await?;

    // Process audio stream
    transcriber.process_audio_stream(rx).await?;

    // Close the transcriber
    transcriber.close().await?;
    let _ = audio_thread.join();

    Ok(())
}


