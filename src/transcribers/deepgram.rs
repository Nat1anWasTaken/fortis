use std::error::Error;
use std::time::Duration;

use deepgram::Deepgram;
use deepgram::common::options::Encoding;
use deepgram::common::options::Options;
use deepgram::common::stream_response::StreamResponse;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::time;

use crate::transcribers::{AudioTranscriber, TranscriptionResult};

/// Deepgram transcription provider implementation
pub struct DeepgramTranscriber {
    client: Deepgram,
    sample_rate: u32,
    channels: u16,
}

impl DeepgramTranscriber {
    /// Create a new Deepgram transcriber instance
    pub fn new(api_key: &str) -> Result<Self, Box<dyn Error>> {
        let client = Deepgram::new(api_key)?;

        Ok(Self {
            client,
            sample_rate: 0,
            channels: 0,
        })
    }

    /// Format and parse a Deepgram response into transcription results
    fn format_response(
        response: &StreamResponse,
    ) -> Vec<TranscriptionResult> {
        let mut results = Vec::new();

        match response {
            StreamResponse::TranscriptResponse { channel, .. } => {
                // Process each alternative (usually just one)
                for alternative in &channel.alternatives {
                    // Build speaker-aware output from words
                    let mut current_speaker: Option<i32> = None;
                    let mut speaker_message = String::new();

                    for word in &alternative.words {
                        // Check if speaker changed
                        if word.speaker != current_speaker {
                            // Save previous speaker's message if any
                            if let Some(speaker_id) = current_speaker {
                                results.push(TranscriptionResult {
                                    transcript: speaker_message.trim().to_string(),
                                    speaker_id: Some(speaker_id),
                                });
                                speaker_message.clear();
                            }
                            current_speaker = word.speaker;
                        }

                        // Add word to current message
                        if !speaker_message.is_empty() {
                            speaker_message.push(' ');
                        }
                        speaker_message.push_str(&word.word);
                    }

                    // Save final speaker's message
                    if let Some(speaker_id) = current_speaker {
                        results.push(TranscriptionResult {
                            transcript: speaker_message.trim().to_string(),
                            speaker_id: Some(speaker_id),
                        });
                    }

                    // If no speaker data, just use the transcript
                    if current_speaker.is_none() && !alternative.transcript.is_empty() {
                        results.push(TranscriptionResult {
                            transcript: alternative.transcript.clone(),
                            speaker_id: None,
                        });
                    }
                }
            }
            StreamResponse::SpeechStartedResponse { .. } => {
                // Optionally log speech detection
            }
            StreamResponse::UtteranceEndResponse { .. } => {
                // Optionally log utterance end
            }
            StreamResponse::TerminalResponse { .. } => {
                results.push(TranscriptionResult {
                    transcript: "Transcription stream ended".to_string(),
                    speaker_id: None,
                });
            }
            _ => {
                // Catch any future StreamResponse variants
            }
        }

        results
    }
}

#[async_trait::async_trait]
impl AudioTranscriber for DeepgramTranscriber {
    async fn initialize(
        &mut self,
        sample_rate: u32,
        channels: u16,
    ) -> Result<(), Box<dyn Error>> {
        self.sample_rate = sample_rate;
        self.channels = channels;
        println!("Streaming transcription started. Speak into your microphone...");
        println!("Press Ctrl+C to stop recording.");
        Ok(())
    }

    async fn close(&mut self) -> Result<(), Box<dyn Error>> {
        Ok(())
    }

    async fn process_audio_stream(
        &mut self,
        mut audio_receiver: UnboundedReceiver<Vec<u8>>,
    ) -> Result<(), Box<dyn Error>> {
        let options = Options::builder()
            .encoding(Encoding::Linear16)
            .diarize(true)
            .build();

        let mut handle = self
            .client
            .transcription()
            .stream_request_with_options(options)
            .sample_rate(self.sample_rate)
            .channels(self.channels)
            .handle()
            .await?;

        let mut keep_alive_interval = time::interval(Duration::from_secs(3));

        loop {
            tokio::select! {
                _ = keep_alive_interval.tick() => {
                    if let Err(err) = handle.keep_alive().await {
                        eprintln!("Keep-alive error: {err}");
                        break;
                    }
                }
                maybe_audio = audio_receiver.recv() => {
                    match maybe_audio {
                        Some(audio_data) => {
                            if let Err(err) = handle.send_data(audio_data).await {
                                eprintln!("Send error: {err}");
                                break;
                            }
                        }
                        None => {
                            // Audio capture ended, finalize the stream
                            if let Err(err) = handle.finalize().await {
                                eprintln!("Finalize error: {err}");
                            }
                            break;
                        }
                    }
                }
                response = handle.receive() => {
                    match response {
                        Some(Ok(result)) => {
                            let results = Self::format_response(&result);
                            for res in results {
                                if let Some(speaker_id) = res.speaker_id {
                                    println!("Speaker {}: {}", speaker_id, res.transcript);
                                } else {
                                    println!("{}", res.transcript);
                                }
                            }
                        }
                        Some(Err(err)) => {
                            eprintln!("Receive error: {err}");
                            break;
                        }
                        None => break,
                    }
                }
            }
        }

        handle.close_stream().await?;
        Ok(())
    }
}
