use ratatui::{
    prelude::*,
    widgets::*,
};

use crate::audio::get_device_name;
use crate::state::{AppState, RecordingState};

/// State for the transcription widget
pub struct TranscriptionWidgetState {
    /// List of transcription messages
    pub transcriptions: Vec<String>,
    /// Current scroll position
    pub scroll_position: usize,
}

impl TranscriptionWidgetState {
    pub fn new() -> Self {
        Self {
            transcriptions: Vec::new(),
            scroll_position: 0,
        }
    }

    /// Add a new transcription message
    pub fn add_transcription(&mut self, message: String) {
        self.transcriptions.push(message);
        // Auto-scroll to bottom when new message arrives
        self.scroll_to_bottom();
    }

    /// Scroll to the bottom of the transcriptions
    pub fn scroll_to_bottom(&mut self) {
        self.scroll_position = 0;
    }

    /// Scroll up in the transcriptions
    pub fn scroll_up(&mut self) {
        self.scroll_position = self.scroll_position.saturating_add(1);
    }

    /// Scroll down in the transcriptions
    pub fn scroll_down(&mut self) {
        self.scroll_position = self.scroll_position.saturating_sub(1);
    }
}

/// Transcription display widget
pub struct TranscriptionWidget;

impl TranscriptionWidget {
    /// Render the transcription widget
    pub fn render(frame: &mut Frame, state: &TranscriptionWidgetState, app_state: &AppState, area: Rect) {
        let text: Vec<Line> = if state.transcriptions.is_empty() {
            vec![Line::from(Span::styled(
                "Waiting for transcriptions...",
                Style::default().fg(Color::DarkGray),
            ))]
        } else {
            state.transcriptions
                .iter()
                .map(|msg| Line::from(msg.clone()))
                .collect()
        };

        // Build title with recording state indicator and timer
        let (state_text, state_color) = match app_state.recording_state() {
            RecordingState::Recording => ("● RECORDING", Color::Red),
            RecordingState::Paused => ("⏸ PAUSED", Color::Yellow),
        };

        let timer_text = app_state.format_recording_time();
        let title = format!(" Transcriptions {} {} ", state_text, timer_text);

        // Get current audio device name
        let device_name = get_device_name(app_state.current_device_index())
            .unwrap_or_else(|_| "Unknown Device".to_string());
        let device_title = format!(" 🎤 {} (D: Change) ", device_name);

        let paragraph = Paragraph::new(text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(Span::styled(title, Style::default().fg(state_color).bold()))
                    .title_top(
                        Line::from(Span::styled(
                            device_title,
                            Style::default().fg(Color::Cyan)
                        ))
                        .right_aligned()
                    )
                    .border_type(BorderType::Rounded)
            )
            .wrap(Wrap { trim: false })
            .scroll((state.scroll_position as u16, 0));

        frame.render_widget(paragraph, area);
    }
}
