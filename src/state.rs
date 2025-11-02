use std::collections::VecDeque;

/// Application state machine
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppState {
    SelectingDevice,
    ReadyToRecord,
    Recording,
    Transcribing,
    Done,
}

/// Application model containing all state
pub struct AppModel {
    /// Current application state
    pub state: AppState,

    /// Available audio devices
    pub devices: Vec<String>,

    /// Currently selected device index
    pub selected_device: usize,

    /// Transcript history (scrollable)
    pub transcript_lines: VecDeque<String>,

    /// Current scroll position in transcript
    pub scroll_position: usize,

    /// Whether the app should continue running
    pub should_exit: bool,

    /// Status message for display
    pub status_message: String,
}

impl AppModel {
    /// Create a new application model with given devices
    pub fn new(devices: Vec<String>) -> Self {
        AppModel {
            state: AppState::SelectingDevice,
            devices,
            selected_device: 0,
            transcript_lines: VecDeque::new(),
            scroll_position: 0,
            should_exit: false,
            status_message: "Ready".to_string(),
        }
    }

    /// Move selection up in device list
    pub fn select_prev_device(&mut self) {
        if self.selected_device > 0 {
            self.selected_device -= 1;
        }
    }

    /// Move selection down in device list
    pub fn select_next_device(&mut self) {
        if self.selected_device < self.devices.len().saturating_sub(1) {
            self.selected_device += 1;
        }
    }

    /// Get the currently selected device name
    pub fn selected_device_name(&self) -> Option<&str> {
        self.devices.get(self.selected_device).map(|s| s.as_str())
    }

    /// Add a transcript line
    pub fn add_transcript_line(&mut self, line: String) {
        self.transcript_lines.push_back(line);
        // Keep reasonable buffer size (last 1000 lines)
        while self.transcript_lines.len() > 1000 {
            self.transcript_lines.pop_front();
        }
        // Scroll to bottom when new content arrives
        self.scroll_position = self.transcript_lines.len().saturating_sub(10);
    }

    /// Scroll transcript up
    pub fn scroll_transcript_up(&mut self) {
        self.scroll_position = self.scroll_position.saturating_sub(1);
    }

    /// Scroll transcript down
    pub fn scroll_transcript_down(&mut self) {
        let max_scroll = self.transcript_lines.len().saturating_sub(10);
        if self.scroll_position < max_scroll {
            self.scroll_position += 1;
        }
    }

    /// Update app state
    pub fn set_state(&mut self, state: AppState) {
        self.state = state;
    }

    /// Update status message
    pub fn set_status(&mut self, message: String) {
        self.status_message = message;
    }

    /// Clear transcript
    pub fn clear_transcript(&mut self) {
        self.transcript_lines.clear();
        self.scroll_position = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_selection() {
        let mut model = AppModel::new(vec!["Device1".into(), "Device2".into(), "Device3".into()]);
        assert_eq!(model.selected_device, 0);

        model.select_next_device();
        assert_eq!(model.selected_device, 1);

        model.select_next_device();
        assert_eq!(model.selected_device, 2);

        model.select_next_device();
        assert_eq!(model.selected_device, 2); // Should not go beyond bounds

        model.select_prev_device();
        assert_eq!(model.selected_device, 1);
    }

    #[test]
    fn test_transcript_management() {
        let mut model = AppModel::new(vec!["Device1".into()]);
        model.add_transcript_line("Line 1".into());
        model.add_transcript_line("Line 2".into());
        model.add_transcript_line("Line 3".into());

        assert_eq!(model.transcript_lines.len(), 3);
    }

    #[test]
    fn test_transcript_scrolling() {
        let mut model = AppModel::new(vec!["Device1".into()]);
        for i in 0..20 {
            model.add_transcript_line(format!("Line {}", i));
        }

        model.scroll_transcript_up();
        assert!(model.scroll_position > 0);

        model.scroll_transcript_down();
    }
}
