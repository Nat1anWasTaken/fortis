use std::error::Error;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

pub mod deepgram;

/// Represents a response from a transcription provider
#[derive(Debug, Clone)]
pub struct TranscriptionResult {
    pub transcript: String,
    pub speaker_id: Option<i32>,
}

/// Trait for audio transcription providers
#[async_trait::async_trait]
pub trait AudioTranscriber: Send + Sync {
    /// Initialize the transcriber with sample rate and channel configuration
    async fn initialize(&mut self, sample_rate: u32, channels: u16) -> Result<(), Box<dyn Error>>;

    /// Close the transcription stream
    async fn close(&mut self) -> Result<(), Box<dyn Error>>;

    /// Process a chunk of audio data from the audio receiver and send results through the result channel
    /// This method handles the main transcription loop
    async fn process_audio_stream(
        &mut self,
        audio_receiver: UnboundedReceiver<Vec<u8>>,
        result_sender: UnboundedSender<TranscriptionResult>,
    ) -> Result<(), Box<dyn Error>>;
}

/// Configuration for creating a transcriber instance
pub enum TranscriberConfig {
    /// Deepgram transcriber configuration
    Deepgram { api_key: String },
}

/// Create a transcriber instance based on the provided configuration
pub fn create_transcriber(
    config: TranscriberConfig,
) -> Result<Box<dyn AudioTranscriber>, Box<dyn Error>> {
    match config {
        TranscriberConfig::Deepgram { api_key } => {
            let transcriber = deepgram::DeepgramTranscriber::new(&api_key)?;
            Ok(Box::new(transcriber))
        }
    }
}
