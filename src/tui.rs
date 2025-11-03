use crossterm::{
    event::{self, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::prelude::*;
use std::io::{self, stdout};

use crate::state::AppState;
use crate::widgets::{
    DeviceDialog, DeviceDialogState, FooterWidget, SettingsDialog, SettingsDialogState,
    TranscriptionMessage, TranscriptionWidget, TranscriptionWidgetState,
};

/// Application UI state for the TUI
pub struct App {
    /// Transcription widget state
    pub transcription_state: TranscriptionWidgetState,
    /// Device selection dialog state (None when closed)
    pub device_dialog_state: Option<DeviceDialogState>,
    /// Settings dialog state (None when closed)
    pub settings_dialog_state: Option<SettingsDialogState>,
}

impl App {
    pub fn new(state: &AppState) -> Self {
        let mut app = Self {
            transcription_state: TranscriptionWidgetState::new(state.auto_scroll_enabled()),
            device_dialog_state: None,
            settings_dialog_state: None,
        };
        app.refresh_from_config(state);
        app
    }

    fn refresh_from_config(&mut self, state: &AppState) {
        self.transcription_state
            .set_auto_scroll(state.auto_scroll_enabled());
    }

    /// Add a new transcription message
    pub fn add_transcription(&mut self, message: TranscriptionMessage) {
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

    /// Move focus to the speaker column for the current message
    pub fn focus_left(&mut self) {
        self.transcription_state.focus_left();
    }

    /// Move focus to the content column for the current message
    pub fn focus_right(&mut self) {
        self.transcription_state.focus_right();
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

    /// Open the settings dialog
    pub fn open_settings_dialog(&mut self, state: &AppState) {
        self.settings_dialog_state = Some(SettingsDialogState::new(state.config()));
        // Ensure no other modal remains open
        self.device_dialog_state = None;
    }

    /// Close the settings dialog
    pub fn close_settings_dialog(&mut self) {
        self.settings_dialog_state = None;
    }

    /// Toggle settings dialog visibility
    pub fn toggle_settings_dialog(&mut self, state: &AppState) {
        if self.settings_dialog_state.is_some() {
            self.close_settings_dialog();
        } else {
            self.open_settings_dialog(state);
        }
    }

    /// Handle keyboard input
    pub fn handle_key_event(&mut self, key: event::KeyEvent, state: &mut AppState) -> bool {
        if key.kind != KeyEventKind::Press {
            return false;
        }

        // Handle settings dialog input if open
        if let Some(dialog_state) = &mut self.settings_dialog_state {
            let result = dialog_state.handle_key_event(key, state.config_mut());
            if result.handled {
                if result.value_changed {
                    self.refresh_from_config(state);
                }
                if result.close {
                    self.close_settings_dialog();
                }
                return true;
            }
        }

        // Handle device dialog input separately
        if let Some(dialog_state) = &mut self.device_dialog_state {
            let handled = match key.code {
                KeyCode::Esc => {
                    self.close_device_dialog();
                    true
                }
                KeyCode::Up => {
                    dialog_state.select_previous();
                    true
                }
                KeyCode::Down => {
                    dialog_state.select_next();
                    true
                }
                KeyCode::Enter => {
                    let selected_device = dialog_state.selected();
                    state.set_device_index(selected_device);
                    self.close_device_dialog();
                    // TODO: Need to restart audio capture with new device
                    true
                }
                _ => false,
            };
            return handled;
        }

        // Handle edit mode input separately
        if self.transcription_state.is_editing() {
            let handled = match key.code {
                KeyCode::Esc => {
                    self.transcription_state.cancel_editing();
                    true
                }
                KeyCode::Enter => {
                    self.transcription_state.apply_edit(state);
                    true
                }
                KeyCode::Backspace => {
                    self.transcription_state.handle_backspace();
                    true
                }
                KeyCode::Left => {
                    self.transcription_state.move_cursor_left();
                    true
                }
                KeyCode::Right => {
                    self.transcription_state.move_cursor_right();
                    true
                }
                KeyCode::Char(c) => {
                    self.transcription_state.handle_char_input(c);
                    true
                }
                _ => false,
            };
            return handled;
        }

        // Normal key handling
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                state.request_quit();
                true
            }
            KeyCode::Char(' ') => {
                state.toggle_recording();
                true
            }
            KeyCode::Char('d') | KeyCode::Char('D') => {
                self.open_device_dialog(state.current_device_index());
                true
            }
            KeyCode::Char('s') | KeyCode::Char('S') => {
                self.toggle_settings_dialog(state);
                true
            }
            KeyCode::Enter => {
                self.transcription_state.start_editing();
                true
            }
            KeyCode::Up => {
                self.scroll_up();
                true
            }
            KeyCode::Down => {
                self.scroll_down();
                true
            }
            KeyCode::Left => {
                self.focus_left();
                true
            }
            KeyCode::Right => {
                self.focus_right();
                true
            }
            _ => false,
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
            Constraint::Min(1),                                           // Main content
            Constraint::Length(if state.compact_mode() { 2 } else { 3 }), // Footer
        ])
        .split(frame.area());

    // Render transcriptions widget
    TranscriptionWidget::render(frame, &mut app.transcription_state, state, chunks[0]);

    // Render footer widget
    FooterWidget::render(frame, chunks[1], state.accent_color(), state.compact_mode());

    // Render device selection dialog if open
    if let Some(dialog_state) = &mut app.device_dialog_state {
        frame.render_stateful_widget(
            DeviceDialog::new(state.accent_color()),
            frame.area(),
            dialog_state,
        );
    }

    if let Some(settings_state) = &mut app.settings_dialog_state {
        frame.render_stateful_widget(
            SettingsDialog {
                manager: state.config(),
                accent: state.accent_color(),
            },
            frame.area(),
            settings_state,
        );
    }
}
