use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

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
        Self {
            should_quit: Arc::new(AtomicBool::new(false)),
            is_paused: Arc::new(AtomicBool::new(false)),
            recording_session: RecordingSession {
                start_time: Instant::now(),
                elapsed_recording_time: Duration::ZERO,
                last_pause_time: None,
            },
            current_device_index: 0,
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
            self.recording_session.elapsed_recording_time + self.recording_session.start_time.elapsed()
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

    /// Set the current audio device index
    pub fn set_device_index(&mut self, index: usize) {
        self.current_device_index = index;
    }
}
