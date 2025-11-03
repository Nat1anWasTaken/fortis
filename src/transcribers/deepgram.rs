use std::error::Error;
use std::time::Duration;

use deepgram::common::options::{Encoding, Language, Model, Options};
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
    model: Option<Model>,
}

impl DeepgramTranscriber {
    /// Create a new Deepgram transcriber instance
    pub fn new(api_key: &str, language_code: &str, model_name: &str) -> Result<Self, Box<dyn Error>> {
        let client = Deepgram::new(api_key)?;

        Ok(Self {
            client,
            sample_rate: 0,
            channels: 0,
            language: parse_language_code(language_code),
            model: parse_model_name(model_name),
        })
    }

    /// Check if text contains CJK (Chinese/Japanese/Korean) characters
    fn is_cjk(text: &str) -> bool {
        text.chars().any(|c| {
            matches!(c,
                '\u{4E00}'..='\u{9FFF}' |  // CJK Unified Ideographs
                '\u{3400}'..='\u{4DBF}' |  // CJK Extension A
                '\u{3040}'..='\u{309F}' |  // Hiragana
                '\u{30A0}'..='\u{30FF}'    // Katakana
            )
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
                    let mut last_was_cjk = false;

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
                                last_was_cjk = false;
                            }
                            current_speaker = word.speaker;
                        }

                        // Add word to current message
                        let current_is_cjk = Self::is_cjk(&word.word);

                        // Add space only if message is not empty and at least one word is non-CJK
                        if !speaker_message.is_empty() && !(last_was_cjk && current_is_cjk) {
                            speaker_message.push(' ');
                        }

                        speaker_message.push_str(&word.word);
                        last_was_cjk = current_is_cjk;
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

        if let Some(model) = self.model.clone() {
            builder = builder.model(model);
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
        "multi" => Some(Language::multi),
        "en" => Some(Language::en),
        "en-US" | "en_us" | "enUS" => Some(Language::en_US),
        "en-GB" | "en_gb" | "enGB" => Some(Language::en_GB),
        "en-AU" | "en_au" | "enAU" => Some(Language::en_AU),
        "en-NZ" | "en_nz" | "enNZ" => Some(Language::en_NZ),
        "en-IN" | "en_in" | "enIN" => Some(Language::en_IN),
        "es" => Some(Language::es),
        "es-419" | "es_419" => Some(Language::es_419),
        "es-LATAM" | "es_latam" | "esLATAM" => Some(Language::es_LATAM),
        "fr" => Some(Language::fr),
        "fr-CA" | "fr_ca" | "frCA" => Some(Language::fr_CA),
        "de" => Some(Language::de),
        "de-CH" | "de_ch" | "deCH" => Some(Language::de_CH),
        "it" => Some(Language::it),
        "pt" => Some(Language::pt),
        "pt-BR" | "pt_br" | "ptBR" => Some(Language::pt_BR),
        "nl" => Some(Language::nl),
        "nl-BE" | "nl_be" | "nlBE" => Some(Language::nl_BE),
        "pl" => Some(Language::pl),
        "ru" => Some(Language::ru),
        "uk" => Some(Language::uk),
        "sv" => Some(Language::sv),
        "sv-SE" | "sv_se" | "svSE" => Some(Language::sv_SE),
        "da" => Some(Language::da),
        "no" => Some(Language::no),
        "fi" => Some(Language::fi),
        "tr" => Some(Language::tr),
        "el" => Some(Language::el),
        "cs" => Some(Language::cs),
        "sk" => Some(Language::sk),
        "hu" => Some(Language::hu),
        "ro" => Some(Language::ro),
        "bg" => Some(Language::bg),
        "et" => Some(Language::et),
        "lv" => Some(Language::lv),
        "lt" => Some(Language::lt),
        "ja" => Some(Language::ja),
        "ko" => Some(Language::ko),
        "ko-KR" | "ko_kr" | "koKR" => Some(Language::ko_KR),
        "zh" => Some(Language::zh),
        "zh-CN" | "zh_cn" | "zhCN" => Some(Language::zh_CN),
        "zh-TW" | "zh_tw" | "zhTW" => Some(Language::zh_TW),
        "zh-Hans" | "zh_hans" | "zhHans" => Some(Language::zh_Hans),
        "zh-Hant" | "zh_hant" | "zhHant" => Some(Language::zh_Hant),
        "hi" => Some(Language::hi),
        "hi-Latn" | "hi_latn" | "hiLatn" => Some(Language::hi_Latn),
        "ta" => Some(Language::ta),
        "th" => Some(Language::th),
        "th-TH" | "th_th" | "thTH" => Some(Language::th_TH),
        "vi" => Some(Language::vi),
        "id" => Some(Language::id),
        "ms" => Some(Language::ms),
        "taq" => Some(Language::taq),
        "ca" => Some(Language::ca),
        _ => None,
    }
}

fn parse_model_name(model: &str) -> Option<Model> {
    match model {
        "nova-3" => Some(Model::Nova3),
        "nova-2" => Some(Model::Nova2),
        "nova-2-general" => Some(Model::Nova2),
        "nova-2-meeting" => Some(Model::Nova2Meeting),
        "nova-2-phonecall" => Some(Model::Nova2Phonecall),
        "nova-2-finance" => Some(Model::Nova2Finance),
        "nova-2-conversationalai" => Some(Model::Nova2Conversationalai),
        "nova-2-voicemail" => Some(Model::Nova2Voicemail),
        "nova-2-video" => Some(Model::Nova2Video),
        "nova-2-medical" => Some(Model::Nova2Medical),
        "nova-2-drivethru" => Some(Model::Nova2Drivethru),
        "nova-2-automotive" => Some(Model::Nova2Automotive),
        "nova-3-medical" => Some(Model::Nova3Medical),
        _ => None,
    }
}
