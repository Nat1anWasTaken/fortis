use std::error::Error;
use std::io;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SampleFormat, StreamConfig};
use deepgram::Deepgram;
use tokio::sync::mpsc::{self, UnboundedSender};
use tokio::time;

use dotenv::dotenv;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv().ok();

    let api_key = std::env::var("DEEPGRAM_API_KEY")
        .unwrap_or_else(|_| "YOUR_DEEPGRAM_API_KEY".to_string());
    let dg = Deepgram::new(&api_key)?;

    let (tx, mut rx) = mpsc::unbounded_channel();
    let audio_thread = std::thread::spawn(move || {
        if let Err(err) = capture_audio_from_mic(tx) {
            eprintln!("Failed to capture audio: {err}");
        }
    });

    let mut handle = dg.transcription().stream_request().handle().await?;
    let mut keep_alive = time::interval(Duration::from_secs(3));

    println!("Streaming transcription started. Speak into your microphone...");

    loop {
        tokio::select! {
            _ = keep_alive.tick() => {
                if let Err(err) = handle.keep_alive().await {
                    eprintln!("Keep-alive error: {err}");
                    break;
                }
            }
            maybe_audio = rx.recv() => {
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
                    Some(Ok(result)) => println!("Transcription: {result:?}"),
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
    let _ = audio_thread.join();

    Ok(())
}

fn capture_audio_from_mic(
    tx: UnboundedSender<Vec<u8>>,
) -> Result<(), Box<dyn Error>> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Failed to find default input device"))?;

    println!("Using device: {}", device.name()?);

    let supported_config = device.default_input_config()?;
    println!("Default config: {:?}", supported_config);

    let stream_config: StreamConfig = supported_config.config();
    let sample_format = supported_config.sample_format();

    let stream = match sample_format {
        SampleFormat::F32 => build_input_stream::<f32>(&device, &stream_config, tx.clone())?,
        SampleFormat::I16 => build_input_stream::<i16>(&device, &stream_config, tx.clone())?,
        SampleFormat::U16 => build_input_stream::<u16>(&device, &stream_config, tx.clone())?,
    };

    stream.play()?;

    std::thread::sleep(Duration::from_secs(60));

    Ok(())
}

trait SampleToBytes: Sample + Copy {
    fn append_to(self, buffer: &mut Vec<u8>);
}

impl SampleToBytes for f32 {
    fn append_to(self, buffer: &mut Vec<u8>) {
        buffer.extend_from_slice(&self.to_le_bytes());
    }
}

impl SampleToBytes for i16 {
    fn append_to(self, buffer: &mut Vec<u8>) {
        buffer.extend_from_slice(&self.to_le_bytes());
    }
}

impl SampleToBytes for u16 {
    fn append_to(self, buffer: &mut Vec<u8>) {
        buffer.extend_from_slice(&self.to_le_bytes());
    }
}

fn build_input_stream<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    tx: UnboundedSender<Vec<u8>>,
) -> Result<cpal::Stream, cpal::BuildStreamError>
where
    T: SampleToBytes + Send + 'static,
{
    device.build_input_stream(
        config,
        move |data: &[T], _| {
            let mut bytes = Vec::with_capacity(data.len() * std::mem::size_of::<T>());
            for &sample in data {
                sample.append_to(&mut bytes);
            }

            if tx.send(bytes).is_err() {
                eprintln!("Audio channel closed; stopping capture");
            }
        },
        move |err| {
            eprintln!("Stream error: {err}");
        },
    )
}
