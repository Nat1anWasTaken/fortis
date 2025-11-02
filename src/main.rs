use std::error::Error;
use std::io;
use std::time::Duration;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SampleFormat, StreamConfig};
use deepgram::Deepgram;
use deepgram::common::options::Encoding;
use deepgram::common::options::Options;
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
    let should_stop = Arc::new(AtomicBool::new(false));
    let should_stop_clone = Arc::clone(&should_stop);

    // Setup Ctrl+C handler
    tokio::spawn(async move {
        if let Ok(_) = tokio::signal::ctrl_c().await {
            should_stop_clone.store(true, Ordering::SeqCst);
        }
    });

    let audio_thread = std::thread::spawn(move || {
        if let Err(err) = capture_audio_from_mic(tx, should_stop) {
            eprintln!("Failed to capture audio: {err}");
        }
    });

    let options = Options::builder()
        .encoding(Encoding::Linear16)
        .diarize(true)
        .build();

    let mut handle = dg
        .transcription()
        .stream_request_with_options(options)
        .sample_rate(48000)
        .channels(1)
        .handle()
        .await?;
    let mut keep_alive = time::interval(Duration::from_secs(3));

    println!("Streaming transcription started. Speak into your microphone...");
    println!("Press Ctrl+C to stop recording.");
    let mut audio_buffer = Vec::new();

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
                        audio_buffer.extend_from_slice(&audio_data);
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

    // Save audio to file
    if !audio_buffer.is_empty() {
        match save_audio_to_file(&audio_buffer) {
            Ok(path) => println!("Audio saved to: {}", path),
            Err(err) => eprintln!("Failed to save audio: {err}"),
        }
    }

    handle.close_stream().await?;
    let _ = audio_thread.join();

    Ok(())
}

fn capture_audio_from_mic(
    tx: UnboundedSender<Vec<u8>>,
    should_stop: Arc<AtomicBool>,
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

    // Keep the stream alive until Ctrl+C is pressed
    while !should_stop.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_millis(100));
    }

    Ok(())
}

trait SampleToBytes: Sample + Copy {
    fn append_to(self, buffer: &mut Vec<u8>);
}

impl SampleToBytes for f32 {
    fn append_to(self, buffer: &mut Vec<u8>) {
        // Convert f32 [-1.0, 1.0] to i16
        let sample = (self.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        buffer.extend_from_slice(&sample.to_le_bytes());
    }
}

impl SampleToBytes for i16 {
    fn append_to(self, buffer: &mut Vec<u8>) {
        buffer.extend_from_slice(&self.to_le_bytes());
    }
}

impl SampleToBytes for u16 {
    fn append_to(self, buffer: &mut Vec<u8>) {
        // Convert u16 to i16 by treating as signed
        let sample = (self as i16).wrapping_add(i16::MIN);
        buffer.extend_from_slice(&sample.to_le_bytes());
    }
}

fn average_samples<T: Sample + Copy>(left: T, right: T) -> T {
    // For numeric types, convert to f32, average, and convert back
    let left_f32 = left.to_f32();
    let right_f32 = right.to_f32();
    let avg = (left_f32 + right_f32) / 2.0;
    T::from(&avg)
}

fn build_input_stream<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    tx: UnboundedSender<Vec<u8>>,
) -> Result<cpal::Stream, cpal::BuildStreamError>
where
    T: SampleToBytes + Send + 'static,
{
    let num_channels = config.channels as usize;

    device.build_input_stream(
        config,
        move |data: &[T], _| {
            let mut bytes = Vec::new();

            // Convert stereo to mono by averaging channels
            if num_channels == 2 {
                for chunk in data.chunks(2) {
                    if chunk.len() == 2 {
                        // Average the two channels
                        let left = chunk[0];
                        let right = chunk[1];
                        let averaged = average_samples(left, right);
                        averaged.append_to(&mut bytes);
                    }
                }
            } else {
                // For mono or other channel counts, just pass through
                for &sample in data {
                    sample.append_to(&mut bytes);
                }
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

fn save_audio_to_file(audio_data: &[u8]) -> Result<String, Box<dyn Error>> {
    let filename = format!(
        "recording_{}.wav",
        chrono::Local::now().format("%Y%m%d_%H%M%S")
    );

    // WAV file parameters
    let sample_rate = 48000u32;
    let num_channels = 2u16;
    let bits_per_sample = 16u16;

    let spec = hound::WavSpec {
        channels: num_channels,
        sample_rate,
        bits_per_sample,
        sample_format: hound::SampleFormat::Int,
    };

    let mut writer = hound::WavWriter::create(&filename, spec)?;

    // Convert byte buffer to i16 samples and write them
    let num_samples = audio_data.len() / 2;
    for i in 0..num_samples {
        let sample = i16::from_le_bytes([audio_data[i * 2], audio_data[i * 2 + 1]]);
        writer.write_sample(sample)?;
    }

    writer.finalize()?;

    Ok(filename)
}
