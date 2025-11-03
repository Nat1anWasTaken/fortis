use std::error::Error;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

use crossterm::event::{Event, EventStream};
use dotenv::dotenv;
use futures::StreamExt;
use tokio::sync::mpsc;
use tokio::time::{interval, Duration, MissedTickBehavior};

mod audio;
mod config;
mod state;
mod transcribers;
mod tui;
mod widgets;

use audio::capture_audio_from_mic_with_device;
use state::AppState;
use transcribers::{create_transcriber, AudioTranscriber, TranscriberConfig};
use tui::{init_terminal, render_ui, restore_terminal, App};
use widgets::TranscriptionMessage;

struct AudioCaptureWorker {
    stop_signal: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl AudioCaptureWorker {
    fn spawn(
        device_index: usize,
        sender: mpsc::UnboundedSender<Vec<u8>>,
        quit_signal: Arc<AtomicBool>,
        pause_signal: Arc<AtomicBool>,
    ) -> Self {
        let worker_stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&worker_stop);
        let quit = Arc::clone(&quit_signal);
        let pause = Arc::clone(&pause_signal);
        let handle = std::thread::spawn(move || {
            if let Err(err) =
                capture_audio_from_mic_with_device(device_index, sender, quit, pause, thread_stop)
            {
                eprintln!("Failed to capture audio: {err}");
            }
        });

        Self {
            stop_signal: worker_stop,
            handle: Some(handle),
        }
    }

    fn restart(
        &mut self,
        device_index: usize,
        sender: mpsc::UnboundedSender<Vec<u8>>,
        quit_signal: Arc<AtomicBool>,
        pause_signal: Arc<AtomicBool>,
    ) {
        self.stop();
        *self = Self::spawn(device_index, sender, quit_signal, pause_signal);
    }

    fn stop(&mut self) {
        if let Some(handle) = self.handle.take() {
            self.stop_signal.store(true, Ordering::SeqCst);
            let _ = handle.join();
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv().ok();

    // Initialize centralized state (single source of truth)
    let mut state = AppState::new();

    // Initialize TUI
    let mut terminal = init_terminal()?;
    let mut app = App::new(&state);

    // Helper function to create and initialize a transcriber
    async fn create_and_init_transcriber(state: &AppState) -> Result<Box<dyn AudioTranscriber>, Box<dyn Error>> {
        // Resolve Deepgram credentials and preferences (config overrides environment)
        let api_key = state
            .deepgram_api_key()
            .or_else(|| std::env::var("DEEPGRAM_API_KEY").ok())
            .unwrap_or_else(|| "YOUR_DEEPGRAM_API_KEY".to_string());
        let language = state.deepgram_language();
        let model = state.deepgram_model();

        // Create transcriber based on configuration
        let config = TranscriberConfig::Deepgram {
            api_key,
            language,
            model,
        };
        let mut transcriber = create_transcriber(config)?;
        transcriber.initialize(48000, 1).await?;
        Ok(transcriber)
    }

    // Create channels for audio and transcription results
    let (mut audio_tx, audio_rx) = mpsc::unbounded_channel();
    let (result_tx, mut result_rx) = mpsc::unbounded_channel();

    let mut audio_worker = AudioCaptureWorker::spawn(
        state.current_device_index(),
        audio_tx.clone(),
        state.quit_handle(),
        state.pause_handle(),
    );

    // Create and initialize initial transcriber
    let mut transcriber = create_and_init_transcriber(&state).await?;

    // Spawn transcription task
    let mut transcription_task = tokio::spawn(async move {
        if let Err(err) = transcriber.process_audio_stream(audio_rx, result_tx).await {
            eprintln!("Transcription error: {err}");
        }
    });

    // Main event loop
    let mut event_stream = EventStream::new();
    let mut needs_redraw = true;

    // Create periodic tick for updating the UI (e.g., recording timer)
    let mut tick = interval(Duration::from_millis(100));
    tick.set_missed_tick_behavior(MissedTickBehavior::Skip);

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
                        let speaker = speaker_id.map(|id| state.get_speaker_name(id));
                        app.add_transcription(TranscriptionMessage::new(speaker, speaker_id, transcript));
                        needs_redraw = true;
                    }

                    // Drain any immediately available transcripts to keep the UI snappy
                    while let Ok(additional) = result_rx.try_recv() {
                        let transcript = additional.transcript;
                        let speaker_id = additional.speaker_id;

                        if transcript != "Transcription stream ended" {
                            let speaker = speaker_id.map(|id| state.get_speaker_name(id));
                            app.add_transcription(TranscriptionMessage::new(speaker, speaker_id, transcript));
                            needs_redraw = true;
                        }
                    }
                }
            }
            _ = tick.tick() => {
                // Periodic tick to update the UI (e.g., recording timer)
                needs_redraw = true;
            }
        }

        if state.take_audio_device_restart_needed() {
            audio_worker.restart(
                state.current_device_index(),
                audio_tx.clone(),
                state.quit_handle(),
                state.pause_handle(),
            );
        }

        if state.take_transcriber_restart_needed() {
            // Abort the old transcription task
            transcription_task.abort();

            // Create new channels
            let (new_audio_tx, new_audio_rx) = mpsc::unbounded_channel();
            let (new_result_tx, new_result_rx) = mpsc::unbounded_channel();

            // Update channel references
            audio_tx = new_audio_tx;
            result_rx = new_result_rx;

            // Restart audio worker with new audio_tx
            audio_worker.restart(
                state.current_device_index(),
                audio_tx.clone(),
                state.quit_handle(),
                state.pause_handle(),
            );

            // Create and initialize new transcriber
            match create_and_init_transcriber(&state).await {
                Ok(mut new_transcriber) => {
                    // Spawn new transcription task
                    transcription_task = tokio::spawn(async move {
                        if let Err(err) = new_transcriber.process_audio_stream(new_audio_rx, new_result_tx).await {
                            eprintln!("Transcription error: {err}");
                        }
                    });
                }
                Err(err) => {
                    eprintln!("Failed to restart transcriber: {err}");
                }
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

    audio_worker.stop();

    // Drop the audio channel so the transcription task can finish
    drop(audio_tx);

    let _ = transcription_task.await;

    Ok(())
}
