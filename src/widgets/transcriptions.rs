use std::collections::VecDeque;

use ratatui::{
    prelude::*,
    widgets::*,
};

use crate::audio::get_device_name;
use crate::state::{AppState, RecordingState};

/// State for the transcription widget
pub struct TranscriptionWidgetState {
    /// List of transcription messages
    transcriptions: VecDeque<String>,
    /// Current scroll position
    pub scroll_position: usize,
}

impl TranscriptionWidgetState {
    /// Maximum number of messages retained in memory to keep rendering cheap
    const MAX_TRANSCRIPTIONS: usize = 2_000;

    pub fn new() -> Self {
        Self {
            transcriptions: VecDeque::new(),
            scroll_position: 0,
        }
    }

    /// Add a new transcription message
    pub fn add_transcription(&mut self, message: String) {
        if self.transcriptions.len() >= Self::MAX_TRANSCRIPTIONS {
            self.transcriptions.pop_front();
            self.clamp_scroll();
        }
        self.transcriptions.push_back(message);
        // Auto-scroll to bottom when new message arrives
        self.scroll_to_bottom();
    }

    /// Scroll to the bottom of the transcriptions
    pub fn scroll_to_bottom(&mut self) {
        self.scroll_position = 0;
    }

    /// Scroll up in the transcriptions
    pub fn scroll_up(&mut self) {
        if !self.transcriptions.is_empty() {
            self.scroll_position = self.scroll_position.saturating_add(1);
            self.clamp_scroll();
        }
    }

    /// Scroll down in the transcriptions
    pub fn scroll_down(&mut self) {
        self.scroll_position = self.scroll_position.saturating_sub(1);
    }

    /// Clamp scroll offset so we never go beyond available history
    fn clamp_scroll(&mut self) {
        let max_scroll = self
            .transcriptions
            .len()
            .saturating_sub(1);
        if self.scroll_position > max_scroll {
            self.scroll_position = max_scroll;
        }
    }
}

/// Transcription display widget
pub struct TranscriptionWidget;

impl TranscriptionWidget {
    /// Render the transcription widget
    pub fn render(frame: &mut Frame, state: &TranscriptionWidgetState, app_state: &AppState, area: Rect) {
        let total = state.transcriptions.len();

        let text: Vec<Line> = if total == 0 {
            vec![Line::from(Span::styled(
                "Waiting for transcriptions...",
                Style::default().fg(Color::DarkGray),
            ))]
        } else {
            // Only materialize the portion of history that fits inside the viewport
            let interior_height = area.height.saturating_sub(2).max(1) as usize;
            let visible_lines = total.min(interior_height);
            let max_scroll = total.saturating_sub(visible_lines);
            let offset_from_bottom = state.scroll_position.min(max_scroll);
            let end_index = total.saturating_sub(offset_from_bottom);
            let start_index = end_index.saturating_sub(visible_lines);

            let mut lines = Vec::with_capacity(visible_lines);
            for idx in start_index..end_index {
                if let Some(msg) = state.transcriptions.get(idx) {
                    lines.push(Line::from(msg.as_str()));
                }
            }
            lines
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
            .wrap(Wrap { trim: false });

        frame.render_widget(paragraph, area);
    }
}
