use std::error::Error;
use std::time::Duration;

use crossterm::event::{Event, EventStream};
use dotenv::dotenv;
use futures::StreamExt;
use tokio::sync::mpsc;
use tokio::time::{interval, MissedTickBehavior};

mod audio;
mod state;
mod transcribers;
mod tui;
mod widgets;

use audio::capture_audio_from_mic_with_device;
use state::AppState;
use transcribers::{create_transcriber, TranscriberConfig};
use tui::{init_terminal, render_ui, restore_terminal, App};
use widgets::TranscriptionMessage;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv().ok();

    // Initialize centralized state (single source of truth)
    let mut state = AppState::new();

    // Initialize TUI
    let mut terminal = init_terminal()?;
    let mut app = App::new();

    // Load API key from environment
    let api_key =
        std::env::var("DEEPGRAM_API_KEY").unwrap_or_else(|_| "YOUR_DEEPGRAM_API_KEY".to_string());

    // Create transcriber based on configuration
    let config = TranscriberConfig::Deepgram { api_key };
    let mut transcriber = create_transcriber(config)?;

    // Create channels for audio and transcription results
    let (audio_tx, audio_rx) = mpsc::unbounded_channel();
    let (result_tx, mut result_rx) = mpsc::unbounded_channel();

    // Get handles for audio capture thread
    let should_stop_audio = state.quit_handle();
    let is_paused_audio = state.pause_handle();

    // Spawn audio capture thread using default device (0)
    let audio_thread = std::thread::spawn(move || {
        if let Err(err) =
            capture_audio_from_mic_with_device(0, audio_tx, should_stop_audio, is_paused_audio)
        {
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
    let mut event_stream = EventStream::new();
    let mut tick = interval(Duration::from_millis(100));
    tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut needs_redraw = true;

    loop {
        tokio::select! {
            biased;
            maybe_event = event_stream.next() => {
                match maybe_event {
                    Some(Ok(Event::Key(key))) => {
                        if app.handle_key_event(key, &mut state) {
                            needs_redraw = true;
                        }
                    }
                    Some(Ok(Event::Resize(_, _))) => {
                        needs_redraw = true;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(err)) => {
                        eprintln!("Event stream error: {err}");
                    }
                    None => break,
                }
            }
            maybe_result = result_rx.recv() => {
                if let Some(transcript_result) = maybe_result {
                    let transcript = transcript_result.transcript;
                    let speaker_id = transcript_result.speaker_id;

                    if transcript != "Transcription stream ended" {
                        let speaker = speaker_id.map(|id| format!("Speaker {}", id));
                        app.add_transcription(TranscriptionMessage::new(speaker, transcript));
                        needs_redraw = true;
                    }

                    // Drain any immediately available transcripts to keep the UI snappy
                    while let Ok(additional) = result_rx.try_recv() {
                        let transcript = additional.transcript;
                        let speaker_id = additional.speaker_id;

                        if transcript != "Transcription stream ended" {
                            let speaker = speaker_id.map(|id| format!("Speaker {}", id));
                            app.add_transcription(TranscriptionMessage::new(speaker, transcript));
                            needs_redraw = true;
                        }
                    }
                }
            }
            _ = tick.tick() => {
                needs_redraw = true;
            }
        }

        if needs_redraw {
            terminal.draw(|frame| render_ui(frame, &mut app, &state))?;
            needs_redraw = false;
        }

        if state.should_quit() {
            break;
        }
    }

    // Restore terminal
    restore_terminal()?;

    // Wait for audio thread and transcription task to complete
    let _ = audio_thread.join();
    let _ = transcription_task.await;

    Ok(())
}
