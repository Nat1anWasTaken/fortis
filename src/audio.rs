use std::error::Error;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use tokio::sync::mpsc::UnboundedSender;

pub fn list_audio_devices() -> Result<Vec<String>, Box<dyn Error>> {
    let host = cpal::default_host();
    let devices = host.input_devices()?;

    let mut device_list = Vec::new();
    for device in devices {
        let name = device.name()?;
        device_list.push(name);
    }

    if device_list.is_empty() {
        return Err("No input devices found".into());
    }

    Ok(device_list)
}

pub fn get_device_name(index: usize) -> Result<String, Box<dyn Error>> {
    let host = cpal::default_host();
    let devices: Vec<cpal::Device> = host.input_devices()?.collect();

    if devices.is_empty() {
        return Err("No input devices found".into());
    }

    devices
        .get(index)
        .ok_or_else(|| "Invalid device index".into())
        .and_then(|device| device.name().map_err(|e| e.into()))
}

fn get_device_by_index(index: usize) -> Result<cpal::Device, Box<dyn Error>> {
    let host = cpal::default_host();
    let devices: Vec<cpal::Device> = host.input_devices()?.collect();

    if devices.is_empty() {
        return Err("No input devices found".into());
    }

    devices
        .into_iter()
        .nth(index)
        .ok_or_else(|| "Invalid device index".into())
}

pub fn capture_audio_from_mic_with_device(
    device_index: usize,
    tx: UnboundedSender<Vec<u8>>,
    should_stop: Arc<AtomicBool>,
    is_paused: Arc<AtomicBool>,
    worker_stop: Arc<AtomicBool>,
) -> Result<(), Box<dyn Error>> {
    let device = get_device_by_index(device_index)?;

    let supported_config = device.default_input_config()?;

    let stream_config: StreamConfig = supported_config.config();
    let sample_format = supported_config.sample_format();

    let stream = match sample_format {
        SampleFormat::F32 => {
            build_input_stream::<f32>(&device, &stream_config, tx.clone(), is_paused.clone())?
        }
        SampleFormat::I16 => {
            build_input_stream::<i16>(&device, &stream_config, tx.clone(), is_paused.clone())?
        }
        SampleFormat::U16 => {
            build_input_stream::<u16>(&device, &stream_config, tx.clone(), is_paused.clone())?
        }
    };

    stream.play()?;

    // Keep the stream alive until should_stop is signaled
    while !should_stop.load(Ordering::SeqCst) && !worker_stop.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_millis(100));
    }

    Ok(())
}

fn build_input_stream<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    tx: UnboundedSender<Vec<u8>>,
    is_paused: Arc<AtomicBool>,
) -> Result<cpal::Stream, cpal::BuildStreamError>
where
    T: cpal::Sample + Send + 'static,
{
    let num_channels = config.channels as usize;
    let channel_closed = Arc::new(AtomicBool::new(false));

    device.build_input_stream(
        config,
        move |data: &[T], _| {
            // Skip processing if paused or channel is closed
            if is_paused.load(Ordering::SeqCst) || channel_closed.load(Ordering::SeqCst) {
                return;
            }

            let mut bytes = Vec::new();

            if num_channels == 2 {
                // Stereo to mono conversion by averaging channels
                for chunk in data.chunks(2) {
                    if chunk.len() == 2 {
                        let left_f32 = chunk[0].to_f32();
                        let right_f32 = chunk[1].to_f32();
                        let avg = (left_f32 + right_f32) / 2.0;
                        let sample = (avg.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
                        bytes.extend_from_slice(&sample.to_le_bytes());
                    }
                }
            } else {
                // Mono: convert samples to i16
                for &sample in data {
                    let sample_f32 = sample.to_f32();
                    let sample_i16 = (sample_f32.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
                    bytes.extend_from_slice(&sample_i16.to_le_bytes());
                }
            }

            // If send fails, mark channel as closed and stop processing
            if tx.send(bytes).is_err() {
                channel_closed.store(true, Ordering::SeqCst);
            }
        },
        move |_err| {
            // Silently ignore stream errors to avoid spamming the terminal in TUI mode
        },
    )
}
