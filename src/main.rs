use std::error::Error;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use dotenv::dotenv;

mod audio;
mod transcribers;
mod ui;
mod state;
mod event_handler;

use audio::{capture_audio_from_mic_with_device, list_audio_devices};
use transcribers::{TranscriberConfig, create_transcriber};
use ui::TerminalUi;
use state::{AppModel, AppState};
use event_handler::{AppEvent, update_model};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv().ok();

    // Get available audio devices
    let device_names = list_audio_devices()?;

    // Initialize TUI and app state
    let mut terminal_ui = TerminalUi::new()?;
    let mut model = AppModel::new(device_names);

    terminal_ui.draw(&model)?;

    // ========================================================================
    // DEVICE SELECTION PHASE
    // ========================================================================
    while model.state == AppState::SelectingDevice {
        if let Ok(Some(event)) = terminal_ui.check_key_press() {
            update_model(&mut model, event);
        }
        terminal_ui.draw(&model)?;
    }

    if model.should_exit {
        return Ok(());
    }

    // ========================================================================
    // READY TO RECORD PHASE
    // ========================================================================
    while model.state == AppState::ReadyToRecord {
        if let Ok(Some(event)) = terminal_ui.check_key_press() {
            update_model(&mut model, event);
        }
        terminal_ui.draw(&model)?;
    }

    if model.should_exit {
        return Ok(());
    }

    // ========================================================================
    // TRANSCRIPTION SETUP
    // ========================================================================

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

    // Setup Ctrl+C handler
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        should_stop_clone.store(true, Ordering::SeqCst);
    });

    // Spawn audio capture thread
    let device_index = model.selected_device;
    let audio_thread = std::thread::spawn(move || {
        if let Err(err) = capture_audio_from_mic_with_device(device_index, audio_tx, should_stop) {
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

    // ========================================================================
    // RECORDING AND TRANSCRIPTION LOOP
    // ========================================================================
    while model.state == AppState::Recording || model.state == AppState::Transcribing {
        // Check for transcription results and update model
        while let Ok(result) = result_rx.try_recv() {
            // Skip end markers, process actual transcript
            if result.transcript != "Transcription stream ended" {
                let text = if result.speaker_id.is_some() {
                    format!("[Speaker {}]: {}", result.speaker_id.unwrap_or(0), result.transcript)
                } else {
                    result.transcript
                };

                // Update model with new transcript
                update_model(&mut model, AppEvent::TranscriptionReceived(text));
            }
        }

        // Update UI
        terminal_ui.draw(&model)?;

        // Check for user input
        if let Ok(Some(event)) = terminal_ui.check_key_press() {
            match event {
                AppEvent::Quit => {
                    model.set_state(AppState::Transcribing);
                    break;
                }
                AppEvent::ScrollUp => {
                    model.scroll_transcript_up();
                }
                AppEvent::ScrollDown => {
                    model.scroll_transcript_down();
                }
                _ => {
                    update_model(&mut model, event);
                }
            }
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // ========================================================================
    // CLEANUP AND SHUTDOWN
    // ========================================================================

    // Wait for audio thread and transcription task to complete
    let _ = audio_thread.join();
    let _ = transcription_task.await;

    // Transition to done state
    model.set_state(AppState::Done);
    model.set_status("Transcription completed successfully!".to_string());
    terminal_ui.draw(&model)?;

    // Keep terminal open briefly to show completion
    tokio::time::sleep(Duration::from_secs(2)).await;

    Ok(())
}
