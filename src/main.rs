use std::error::Error;
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tokio::sync::mpsc;
use dotenv::dotenv;

mod audio;
mod transcribers;

use audio::{capture_audio_from_mic_with_device, list_audio_devices};
use transcribers::{TranscriberConfig, create_transcriber};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv().ok();

    println!("=== Fortis Audio Transcription ===\n");

    // Get available audio devices
    let device_names = list_audio_devices()?;

    // Display and select audio device
    println!("Available audio devices:");
    for (index, device) in device_names.iter().enumerate() {
        println!("  {}. {}", index, device);
    }

    print!("\nSelect audio device (enter number): ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let selected_device: usize = input.trim().parse()
        .map_err(|_| "Invalid device number")?;

    if selected_device >= device_names.len() {
        return Err("Invalid device number".into());
    }

    println!("\nSelected device: {}", device_names[selected_device]);
    println!("\nPress Enter to start recording...");

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    println!("\n=== Recording Started ===");
    println!("Press Ctrl+C to stop recording and finish transcription\n");

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
        println!("\n\n=== Stopping Recording ===");
        should_stop_clone.store(true, Ordering::SeqCst);
    });

    // Spawn audio capture thread
    let audio_thread = std::thread::spawn(move || {
        if let Err(err) = capture_audio_from_mic_with_device(selected_device, audio_tx, should_stop) {
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

    // Process transcription results
    loop {
        tokio::select! {
            result = result_rx.recv() => {
                match result {
                    Some(transcript_result) => {
                        // Skip end markers
                        if transcript_result.transcript != "Transcription stream ended" {
                            let text = if let Some(speaker_id) = transcript_result.speaker_id {
                                format!("[Speaker {}]: {}", speaker_id, transcript_result.transcript)
                            } else {
                                transcript_result.transcript
                            };
                            println!("{}", text);
                        }
                    }
                    None => {
                        // Channel closed, transcription finished
                        break;
                    }
                }
            }
            _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)) => {
                // Continue loop
            }
        }
    }

    // Wait for audio thread and transcription task to complete
    let _ = audio_thread.join();
    let _ = transcription_task.await;

    println!("\n=== Transcription Complete ===");

    Ok(())
}
