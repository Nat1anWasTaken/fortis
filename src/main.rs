use std::error::Error;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use dotenv::dotenv;
use crossterm::event::Event;

mod audio;
mod transcribers;
mod tui;

use audio::capture_audio_from_mic_with_device;
use transcribers::{TranscriberConfig, create_transcriber};
use tui::{App, init_terminal, restore_terminal, render_ui, poll_events};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv().ok();

    // Initialize TUI
    let mut terminal = init_terminal()?;
    let mut app = App::new();

    // Load API key from environment
    let api_key = std::env::var("DEEPGRAM_API_KEY")
        .unwrap_or_else(|_| "YOUR_DEEPGRAM_API_KEY".to_string());

    // Create transcriber based on configuration
    let config = TranscriberConfig::Deepgram { api_key };
    let mut transcriber = create_transcriber(config)?;

    // Create channels for audio and transcription results
    let (audio_tx, audio_rx) = mpsc::unbounded_channel();
    let (result_tx, mut result_rx) = mpsc::unbounded_channel();

    let should_stop = Arc::new(AtomicBool::new(false));
    let should_stop_clone = Arc::clone(&should_stop);
    let should_stop_audio = Arc::clone(&should_stop);

    // Spawn audio capture thread using default device (0)
    let audio_thread = std::thread::spawn(move || {
        if let Err(err) = capture_audio_from_mic_with_device(0, audio_tx, should_stop_audio) {
            eprintln!("Failed to capture audio: {err}");
        }
    });

    // Initialize transcriber with sample rate and channels
    transcriber.initialize(48000, 1).await?;

    // Spawn transcription task
    let transcription_task = tokio::spawn(async move {
        if let Err(err) = transcriber.process_audio_stream(audio_rx, result_tx).await {
            eprintln!("Transcription error: {err}");
        }
    });

    // Main event loop
    loop {
        // Render the UI
        terminal.draw(|frame| render_ui(frame, &app))?;

        // Poll for events with a short timeout
        if let Ok(Some(event)) = poll_events(Duration::from_millis(50)) {
            if let Event::Key(key) = event {
                app.handle_key_event(key);
            }
        }

        // Check for transcription results
        while let Ok(transcript_result) = result_rx.try_recv() {
            // Skip end markers
            if transcript_result.transcript != "Transcription stream ended" {
                let text = if let Some(speaker_id) = transcript_result.speaker_id {
                    format!("[Speaker {}]: {}", speaker_id, transcript_result.transcript)
                } else {
                    transcript_result.transcript
                };
                app.add_transcription(text);
            }
        }

        // Check if we should quit
        if app.should_quit {
            should_stop_clone.store(true, Ordering::SeqCst);
            break;
        }

        // Small sleep to prevent busy-waiting
        tokio::time::sleep(Duration::from_millis(16)).await; // ~60 FPS
    }

    // Restore terminal
    restore_terminal()?;

    // Wait for audio thread and transcription task to complete
    let _ = audio_thread.join();
    let _ = transcription_task.await;

    Ok(())
}
