use std::error::Error;
use std::time::Duration;

use deepgram::common::options::{Encoding, Language, Options};
use deepgram::common::stream_response::StreamResponse;
use deepgram::Deepgram;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::time;

use crate::transcribers::{AudioTranscriber, TranscriptionResult};

/// Deepgram transcription provider implementation
pub struct DeepgramTranscriber {
    client: Deepgram,
    sample_rate: u32,
    channels: u16,
    language: Option<Language>,
}

impl DeepgramTranscriber {
    /// Create a new Deepgram transcriber instance
    pub fn new(api_key: &str, language_code: &str) -> Result<Self, Box<dyn Error>> {
        let client = Deepgram::new(api_key)?;

        Ok(Self {
            client,
            sample_rate: 0,
            channels: 0,
            language: parse_language_code(language_code),
        })
    }

    /// Format and parse a Deepgram response into transcription results
    fn format_response(response: &StreamResponse) -> Vec<TranscriptionResult> {
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
    async fn initialize(&mut self, sample_rate: u32, channels: u16) -> Result<(), Box<dyn Error>> {
        self.sample_rate = sample_rate;
        self.channels = channels;
        Ok(())
    }

    async fn close(&mut self) -> Result<(), Box<dyn Error>> {
        Ok(())
    }

    async fn process_audio_stream(
        &mut self,
        mut audio_receiver: UnboundedReceiver<Vec<u8>>,
        result_sender: UnboundedSender<TranscriptionResult>,
    ) -> Result<(), Box<dyn Error>> {
        let mut builder = Options::builder()
            .encoding(Encoding::Linear16)
            .diarize(true);

        if let Some(language) = self.language.clone() {
            builder = builder.language(language);
        }

        let options = builder.build();

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
                            // Send each result to the UI through the channel
                            for transcription_result in results {
                                if let Err(err) = result_sender.send(transcription_result) {
                                    eprintln!("Failed to send transcription result: {err}");
                                    break;
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

fn parse_language_code(code: &str) -> Option<Language> {
    match code {
        "en-US" | "en_us" | "enUS" => Some(Language::en_US),
        "en-GB" | "en_gb" | "enGB" => Some(Language::en_GB),
        "en" => Some(Language::en),
        "es" => Some(Language::es),
        "es-LATAM" | "es_latam" | "esLATAM" => Some(Language::es_LATAM),
        "fr" => Some(Language::fr),
        "de" => Some(Language::de),
        "it" => Some(Language::it),
        "pt-BR" | "pt_br" | "ptBR" => Some(Language::pt_BR),
        "hi" => Some(Language::hi),
        "ja" => Some(Language::ja),
        "ko" => Some(Language::ko),
        _ => None,
    }
}
