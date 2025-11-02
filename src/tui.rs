use std::io::{self, stdout};
use std::time::Duration;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::prelude::*;

use crate::state::AppState;
use crate::widgets::{DeviceDialog, DeviceDialogState, TranscriptionWidget, TranscriptionWidgetState, FooterWidget};

/// Application UI state for the TUI
pub struct App {
    /// Transcription widget state
    pub transcription_state: TranscriptionWidgetState,
    /// Device selection dialog state (None when closed)
    pub device_dialog_state: Option<DeviceDialogState>,
}

impl App {
    pub fn new() -> Self {
        Self {
            transcription_state: TranscriptionWidgetState::new(),
            device_dialog_state: None,
        }
    }

    /// Add a new transcription message
    pub fn add_transcription(&mut self, message: String) {
        self.transcription_state.add_transcription(message);
    }

    /// Scroll up in the transcriptions
    pub fn scroll_up(&mut self) {
        self.transcription_state.scroll_up();
    }

    /// Scroll down in the transcriptions
    pub fn scroll_down(&mut self) {
        self.transcription_state.scroll_down();
    }

    /// Open the device selection dialog
    pub fn open_device_dialog(&mut self, current_device: usize) {
        // Load available devices
        if let Ok(devices) = crate::audio::list_audio_devices() {
            self.device_dialog_state = Some(DeviceDialogState::new(devices, current_device));
        }
    }

    /// Close the device selection dialog
    pub fn close_device_dialog(&mut self) {
        self.device_dialog_state = None;
    }

    /// Handle keyboard input
    pub fn handle_key_event(&mut self, key: event::KeyEvent, state: &mut AppState) {
        if key.kind != KeyEventKind::Press {
            return;
        }

        // Handle device dialog input separately
        if let Some(dialog_state) = &mut self.device_dialog_state {
            match key.code {
                KeyCode::Esc => {
                    self.close_device_dialog();
                }
                KeyCode::Up => {
                    dialog_state.select_previous();
                }
                KeyCode::Down => {
                    dialog_state.select_next();
                }
                KeyCode::Enter => {
                    let selected_device = dialog_state.selected();
                    state.set_device_index(selected_device);
                    self.close_device_dialog();
                    // TODO: Need to restart audio capture with new device
                }
                _ => {}
            }
            return;
        }

        // Normal key handling
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                state.request_quit();
            }
            KeyCode::Char(' ') => {
                state.toggle_recording();
            }
            KeyCode::Char('d') | KeyCode::Char('D') => {
                self.open_device_dialog(state.current_device_index());
            }
            KeyCode::Up => {
                self.scroll_up();
            }
            KeyCode::Down => {
                self.scroll_down();
            }
            _ => {}
        }
    }
}

/// Initialize the terminal for TUI mode
pub fn init_terminal() -> io::Result<Terminal<CrosstermBackend<std::io::Stdout>>> {
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout());
    Terminal::new(backend)
}

/// Restore the terminal to normal mode
pub fn restore_terminal() -> io::Result<()> {
    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}

/// Render the UI
pub fn render_ui(frame: &mut Frame, app: &mut App, state: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),      // Main content
            Constraint::Length(3),    // Footer
        ])
        .split(frame.area());

    // Render transcriptions widget
    TranscriptionWidget::render(frame, &app.transcription_state, state, chunks[0]);

    // Render footer widget
    FooterWidget::render(frame, chunks[1]);

    // Render device selection dialog if open
    if let Some(dialog_state) = &mut app.device_dialog_state {
        frame.render_stateful_widget(DeviceDialog, frame.area(), dialog_state);
    }
}

/// Poll for keyboard events with timeout
pub fn poll_events(timeout: Duration) -> io::Result<Option<Event>> {
    if event::poll(timeout)? {
        Ok(Some(event::read()?))
    } else {
        Ok(None)
    }
}
