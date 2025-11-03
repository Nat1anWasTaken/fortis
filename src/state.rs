use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ratatui::style::Color;

use crate::config::{ConfigField, ConfigManager, SelectOption};

/// Recording state
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RecordingState {
    Recording,
    Paused,
}

/// Centralized application state that serves as the single source of truth
pub struct AppState {
    /// Whether the application should quit
    should_quit: Arc<AtomicBool>,
    /// Whether recording is paused
    is_paused: Arc<AtomicBool>,
    /// Recording session data
    recording_session: RecordingSession,
    /// Current audio device index
    current_device_index: usize,
    /// Cached device name (to avoid expensive system calls every frame)
    current_device_name: String,
    /// Speaker ID to custom name mapping
    speaker_map: HashMap<i32, String>,
    /// Application configuration manager
    config: ConfigManager,
}

/// Recording session tracking
struct RecordingSession {
    /// Recording start time
    start_time: Instant,
    /// Total elapsed recording time (excluding paused periods)
    elapsed_recording_time: Duration,
    /// Time when last paused (if currently paused)
    last_pause_time: Option<Instant>,
}

impl AppState {
    pub fn new() -> Self {
        let mut config = ConfigManager::with_default_schema();
        let (current_device_index, current_device_name) =
            Self::resolve_audio_device(&mut config);

        Self {
            should_quit: Arc::new(AtomicBool::new(false)),
            is_paused: Arc::new(AtomicBool::new(false)),
            recording_session: RecordingSession {
                start_time: Instant::now(),
                elapsed_recording_time: Duration::ZERO,
                last_pause_time: None,
            },
            current_device_index,
            current_device_name,
            speaker_map: HashMap::new(),
            config,
        }
    }

    /// Get a handle for checking if the app should quit (for other threads)
    pub fn quit_handle(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.should_quit)
    }

    /// Get a handle for checking pause state (for other threads)
    pub fn pause_handle(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.is_paused)
    }

    /// Signal that the app should quit
    pub fn request_quit(&self) {
        self.should_quit.store(true, Ordering::SeqCst);
    }

    /// Check if the app should quit
    pub fn should_quit(&self) -> bool {
        self.should_quit.load(Ordering::SeqCst)
    }

    /// Toggle recording state between Recording and Paused
    pub fn toggle_recording(&mut self) {
        let currently_paused = self.is_paused.load(Ordering::SeqCst);

        if currently_paused {
            // Resuming: add the time we were recording before the pause
            if let Some(pause_time) = self.recording_session.last_pause_time {
                self.recording_session.elapsed_recording_time +=
                    pause_time.duration_since(self.recording_session.start_time);
            }
            // Reset start time for the new recording session
            self.recording_session.start_time = Instant::now();
            self.recording_session.last_pause_time = None;
            self.is_paused.store(false, Ordering::SeqCst);
        } else {
            // Pausing: record the pause time
            self.recording_session.last_pause_time = Some(Instant::now());
            self.is_paused.store(true, Ordering::SeqCst);
        }
    }

    /// Get current recording state
    pub fn recording_state(&self) -> RecordingState {
        if self.is_paused.load(Ordering::SeqCst) {
            RecordingState::Paused
        } else {
            RecordingState::Recording
        }
    }

    /// Get the current recording time
    pub fn get_recording_time(&self) -> Duration {
        if self.is_paused.load(Ordering::SeqCst) {
            // Paused: return just the elapsed time
            self.recording_session.elapsed_recording_time
        } else {
            // Currently recording: add current session time to elapsed time
            self.recording_session.elapsed_recording_time
                + self.recording_session.start_time.elapsed()
        }
    }

    /// Format recording time as HH:MM:SS
    pub fn format_recording_time(&self) -> String {
        let total_secs = self.get_recording_time().as_secs();
        let hours = total_secs / 3600;
        let minutes = (total_secs % 3600) / 60;
        let seconds = total_secs % 60;
        format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
    }

    /// Get the current audio device index
    pub fn current_device_index(&self) -> usize {
        self.current_device_index
    }

    /// Get the cached current device name (no system calls)
    pub fn current_device_name(&self) -> &str {
        &self.current_device_name
    }

    /// Set the current audio device index
    pub fn set_device_index(&mut self, index: usize) {
        self.current_device_index = index;
        // Update cached device name
        self.current_device_name =
            crate::audio::get_device_name(index).unwrap_or_else(|_| "Unknown Device".to_string());
        if let Err(err) = self
            .config
            .set_select("audio.input.device", &self.current_device_name)
        {
            eprintln!(
                "Warning: failed to persist selected audio device '{}': {err}",
                self.current_device_name
            );
        }
    }

    /// Get the display name for a speaker ID
    pub fn get_speaker_name(&self, speaker_id: i32) -> String {
        self.speaker_map
            .get(&speaker_id)
            .cloned()
            .unwrap_or_else(|| format!("Speaker {}", speaker_id))
    }

    /// Set a custom name for a speaker ID
    pub fn set_speaker_name(&mut self, speaker_id: i32, name: String) {
        self.speaker_map.insert(speaker_id, name);
    }

    /// Check if a speaker has a custom name
    pub fn has_custom_name(&self, speaker_id: i32) -> bool {
        self.speaker_map.contains_key(&speaker_id)
    }

    /// Access configuration manager (immutable)
    pub fn config(&self) -> &ConfigManager {
        &self.config
    }

    /// Access configuration manager (mutable)
    pub fn config_mut(&mut self) -> &mut ConfigManager {
        &mut self.config
    }

    /// Determine if the UI should auto-scroll when new transcripts arrive.
    pub fn auto_scroll_enabled(&self) -> bool {
        self.config
            .bool_value("ui.behavior.auto_scroll")
            .unwrap_or(true)
    }

    /// Whether the compact layout option is enabled.
    pub fn compact_mode(&self) -> bool {
        self.config
            .bool_value("ui.behavior.compact_mode")
            .unwrap_or(false)
    }

    /// Current accent color, adjusted by the configured brightness multiplier.
    pub fn accent_color(&self) -> Color {
        let base = self
            .config
            .select_value("ui.theme.accent_color")
            .unwrap_or_else(|_| "blue".to_string());
        let brightness = self
            .config
            .number_value("ui.theme.brightness")
            .unwrap_or(1.0);

        let (r, g, b) = match base.as_str() {
            "cyan" => (0, 188, 242),
            "magenta" => (216, 46, 154),
            "amber" => (255, 179, 71),
            "green" => (0, 200, 117),
            _ => (59, 130, 246), // blue
        };

        let adjust = |component: u8| -> u8 {
            ((component as f64 * brightness).clamp(0.0, 255.0)).round() as u8
        };

        Color::Rgb(adjust(r), adjust(g), adjust(b))
    }

    /// Optional Deepgram API key stored in configuration.
    pub fn deepgram_api_key(&self) -> Option<String> {
        self.config
            .text_value("transcriber.deepgram.api_key")
            .ok()
            .and_then(|value| {
                let trimmed = value.trim().to_string();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed)
                }
            })
    }

    /// Configured Deepgram language code (defaults to en-US).
    pub fn deepgram_language(&self) -> String {
        self.config
            .select_value("transcriber.deepgram.language")
            .unwrap_or_else(|_| "en-US".to_string())
    }

    /// Synchronize the active audio device with the persisted configuration.
    pub fn sync_audio_device_from_config(&mut self) {
        let (index, name) = Self::resolve_audio_device(&mut self.config);
        self.current_device_index = index;
        self.current_device_name = name;
    }

    fn resolve_audio_device(config: &mut ConfigManager) -> (usize, String) {
        const DEVICE_KEY: &str = "audio.input.device";

        match crate::audio::list_audio_devices() {
            Ok(devices) => {
                if devices.is_empty() {
                    let placeholder_value = "__no_devices__".to_string();
                    let options = vec![SelectOption::new(
                        placeholder_value.clone(),
                        "No input devices detected",
                    )];
                    if let Err(err) = config.update_select_options(DEVICE_KEY, options, Some(placeholder_value)) {
                        eprintln!(
                            "Warning: failed to refresh audio device options after empty enumeration: {err}"
                        );
                    }
                    return (0, "Unknown Device".to_string());
                }

                let new_options: Vec<SelectOption> = devices
                    .iter()
                    .map(|name| SelectOption::new(name.clone(), name.clone()))
                    .collect();
                let default_candidate = new_options.first().map(|opt| opt.value.clone());

                let needs_option_refresh = config
                    .entry(DEVICE_KEY)
                    .ok()
                    .map(|entry| match &entry.field {
                        ConfigField::Select { default, options } => {
                            if options.len() != new_options.len() {
                                return true;
                            }
                            if options
                                .iter()
                                .zip(new_options.iter())
                                .any(|(existing, updated)| {
                                    existing.value != updated.value || existing.label != updated.label
                                })
                            {
                                return true;
                            }
                            if let Some(default_value) = &default_candidate {
                                default_value != default
                            } else {
                                false
                            }
                        }
                        _ => true,
                    })
                    .unwrap_or(true);

                if needs_option_refresh {
                    if let Err(err) = config.update_select_options(
                        DEVICE_KEY,
                        new_options.clone(),
                        default_candidate.clone(),
                    ) {
                        eprintln!(
                            "Warning: failed to refresh audio device options: {err}"
                        );
                    }
                }

                let desired = config
                    .select_value(DEVICE_KEY)
                    .unwrap_or_else(|_| default_candidate.clone().unwrap_or_else(|| devices[0].clone()));

                if let Some(index) = devices.iter().position(|name| name == &desired) {
                    (index, desired)
                } else {
                    let fallback = devices[0].clone();
                    if let Err(err) = config.set_select(DEVICE_KEY, &fallback) {
                        eprintln!(
                            "Warning: failed to reset audio device selection to '{}': {err}",
                            fallback
                        );
                    }
                    (0, fallback)
                }
            }
            Err(err) => {
                eprintln!("Warning: failed to enumerate audio devices: {err}");
                let placeholder_value = "__no_devices__".to_string();
                let options = vec![SelectOption::new(
                    placeholder_value.clone(),
                    "Audio device enumeration failed",
                )];
                if let Err(update_err) =
                    config.update_select_options(DEVICE_KEY, options, Some(placeholder_value))
                {
                    eprintln!(
                        "Warning: failed to refresh audio device options after enumeration error: {update_err}"
                    );
                }
                (0, "Unknown Device".to_string())
            }
        }
    }
}
