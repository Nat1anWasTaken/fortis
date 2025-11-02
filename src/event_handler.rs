use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crate::state::{AppModel, AppState};

/// Event types for the application
#[derive(Debug, Clone, PartialEq)]
pub enum AppEvent {
    /// User pressed up arrow or 'w'
    SelectPrev,
    /// User pressed down arrow or 's'
    SelectNext,
    /// User pressed enter to confirm
    Confirm,
    /// User pressed 'r' to start recording
    StartRecording,
    /// User pressed 'q' to quit
    Quit,
    /// User pressed Page Up to scroll up
    ScrollUp,
    /// User pressed Page Down to scroll down
    ScrollDown,
    /// Transcription result received
    TranscriptionReceived(String),
    /// Recording finished
    RecordingFinished,
    /// Transcription finished
    TranscriptionFinished,
    /// Timeout (no user input)
    Tick,
}

/// Convert crossterm events to app events
pub fn handle_key_event(event: KeyEvent) -> Option<AppEvent> {
    let code = event.code;
    let mods = event.modifiers;

    match code {
        KeyCode::Up | KeyCode::Char('w') | KeyCode::Char('W') => Some(AppEvent::SelectPrev),
        KeyCode::Down | KeyCode::Char('s') | KeyCode::Char('S') => Some(AppEvent::SelectNext),
        KeyCode::Enter => Some(AppEvent::Confirm),
        KeyCode::Char('r') | KeyCode::Char('R') => Some(AppEvent::StartRecording),
        KeyCode::Char('q') | KeyCode::Char('Q') => Some(AppEvent::Quit),
        KeyCode::Char('c') if mods.contains(KeyModifiers::CONTROL) => Some(AppEvent::Quit),
        KeyCode::PageUp => Some(AppEvent::ScrollUp),
        KeyCode::PageDown => Some(AppEvent::ScrollDown),
        _ => None,
    }
}

/// Handle application events and update the model accordingly
pub fn update_model(model: &mut AppModel, event: AppEvent) {

    match event {
        AppEvent::SelectPrev => {
            if model.state == AppState::SelectingDevice {
                model.select_prev_device();
            } else if model.state == AppState::Done {
                model.scroll_transcript_up();
            }
        }
        AppEvent::SelectNext => {
            if model.state == AppState::SelectingDevice {
                model.select_next_device();
            } else if model.state == AppState::Done {
                model.scroll_transcript_down();
            }
        }
        AppEvent::Confirm => {
            if model.state == AppState::SelectingDevice {
                model.set_state(AppState::ReadyToRecord);
                model.set_status("Ready to record. Press 'r' to start.".to_string());
            }
        }
        AppEvent::StartRecording => {
            if model.state == AppState::ReadyToRecord {
                model.set_state(AppState::Recording);
                model.clear_transcript();
                model.set_status("Recording... Press 'q' to stop.".to_string());
            }
        }
        AppEvent::ScrollUp => {
            model.scroll_transcript_up();
        }
        AppEvent::ScrollDown => {
            model.scroll_transcript_down();
        }
        AppEvent::TranscriptionReceived(text) => {
            if model.state == AppState::Recording || model.state == AppState::Transcribing {
                model.add_transcript_line(text);
                model.set_status("Transcribing...".to_string());
            }
        }
        AppEvent::RecordingFinished => {
            if model.state == AppState::Recording {
                model.set_state(AppState::Transcribing);
                model.set_status("Waiting for transcription to complete...".to_string());
            }
        }
        AppEvent::TranscriptionFinished => {
            if model.state == AppState::Transcribing {
                model.set_state(AppState::Done);
                model.set_status("Done. Press 'q' to quit or select new device.".to_string());
            }
        }
        AppEvent::Quit => {
            model.should_exit = true;
        }
        AppEvent::Tick => {
            // No action needed
        }
    }
}

/// Get help text based on current app state
pub fn get_help_text(state: AppState) -> String {
    match state {
        AppState::SelectingDevice => {
            "↑/W: Up | ↓/S: Down | Enter: Select | Q: Quit".to_string()
        }
        AppState::ReadyToRecord => {
            "R: Start Recording | Q: Quit".to_string()
        }
        AppState::Recording => {
            "Recording in progress... Q: Stop".to_string()
        }
        AppState::Transcribing => {
            "Transcribing... Please wait".to_string()
        }
        AppState::Done => {
            "PgUp/PgDn: Scroll | Q: Quit | U: Use Another Device".to_string()
        }
    }
}

/// Get status message based on current app state and model
pub fn get_status_message(model: &AppModel) -> String {
    let device = model
        .selected_device_name()
        .unwrap_or("No device selected");
    let state_str = match model.state {
        AppState::SelectingDevice => "Selecting Device",
        AppState::ReadyToRecord => "Ready to Record",
        AppState::Recording => "Recording",
        AppState::Transcribing => "Transcribing",
        AppState::Done => "Done",
    };

    format!("{} | Device: {}", state_str, device)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_event_handling() {
        let event = KeyEvent::new(KeyCode::Up, KeyModifiers::empty());
        assert_eq!(handle_key_event(event), Some(AppEvent::SelectPrev));

        let event = KeyEvent::new(KeyCode::Down, KeyModifiers::empty());
        assert_eq!(handle_key_event(event), Some(AppEvent::SelectNext));

        let event = KeyEvent::new(KeyCode::Enter, KeyModifiers::empty());
        assert_eq!(handle_key_event(event), Some(AppEvent::Confirm));

        let event = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::empty());
        assert_eq!(handle_key_event(event), Some(AppEvent::Quit));
    }

    #[test]
    fn test_state_transitions() {
        let mut model = AppModel::new(vec!["Device1".into()]);
        assert_eq!(model.state, AppState::SelectingDevice);

        update_model(&mut model, AppEvent::Confirm);
        assert_eq!(model.state, AppState::ReadyToRecord);

        update_model(&mut model, AppEvent::StartRecording);
        assert_eq!(model.state, AppState::Recording);
    }
}
