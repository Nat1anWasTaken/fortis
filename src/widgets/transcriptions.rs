use std::collections::VecDeque;

use ratatui::{prelude::*, widgets::*};

use crate::audio::get_device_name;
use crate::state::{AppState, RecordingState};

#[derive(Debug, Clone)]
pub struct TranscriptionMessage {
    pub speaker: Option<String>,
    pub content: String,
}

impl TranscriptionMessage {
    pub fn new(speaker: Option<String>, content: String) -> Self {
        Self { speaker, content }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FocusSegment {
    Speaker,
    Message,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FocusLocation {
    message_index: usize,
    segment: FocusSegment,
}

/// State for the transcription widget
pub struct TranscriptionWidgetState {
    /// List of transcription messages
    transcriptions: VecDeque<TranscriptionMessage>,
    /// Current scroll position (offset from bottom)
    pub scroll_position: usize,
    /// Currently focused message segment
    focus: Option<FocusLocation>,
    /// Height of the inner viewport (rows available for messages)
    viewport_height: usize,
}

impl TranscriptionWidgetState {
    /// Maximum number of messages retained in memory to keep rendering cheap
    const MAX_TRANSCRIPTIONS: usize = 2_000;

    pub fn new() -> Self {
        Self {
            transcriptions: VecDeque::new(),
            scroll_position: 0,
            focus: None,
            viewport_height: 0,
        }
    }

    /// Add a new transcription message
    pub fn add_transcription(&mut self, message: TranscriptionMessage) {
        if self.transcriptions.len() >= Self::MAX_TRANSCRIPTIONS {
            self.transcriptions.pop_front();
            self.handle_removed_front();
            self.clamp_scroll();
        }

        self.transcriptions.push_back(message);

        self.ensure_focus_valid();
        self.ensure_focus_visible();
    }

    /// Move focus to the previous message row
    pub fn scroll_up(&mut self) {
        self.focus_prev_row();
    }

    /// Move focus to the next message row
    pub fn scroll_down(&mut self) {
        self.focus_next_row();
    }

    /// Move focus within the current message toward the speaker column
    pub fn focus_left(&mut self) {
        self.ensure_focus_valid();

        if let Some(mut focus) = self.focus {
            if matches!(focus.segment, FocusSegment::Message)
                && self.message_has_speaker(focus.message_index)
            {
                focus.segment = FocusSegment::Speaker;
                self.focus = Some(focus);
            }
        }

        self.ensure_focus_visible();
    }

    /// Move focus within the current message toward the content column
    pub fn focus_right(&mut self) {
        self.ensure_focus_valid();

        if let Some(mut focus) = self.focus {
            if matches!(focus.segment, FocusSegment::Speaker) {
                focus.segment = FocusSegment::Message;
                self.focus = Some(focus);
            }
        }

        self.ensure_focus_visible();
    }

    /// Update viewport height (number of rows available for messages)
    pub fn update_viewport_height(&mut self, height: usize) {
        self.viewport_height = height.max(1);
        self.clamp_scroll();
        self.ensure_focus_visible();
    }

    fn focus_prev_row(&mut self) {
        if self.transcriptions.is_empty() {
            return;
        }

        self.ensure_focus_valid();

        if let Some(current) = self.focus {
            if current.message_index == 0 {
                self.focus = Some(FocusLocation {
                    message_index: 0,
                    segment: self.resolve_segment_for_message(0, current.segment),
                });
            } else {
                let new_index = current.message_index - 1;
                let segment = self.resolve_segment_for_message(new_index, current.segment);
                self.focus = Some(FocusLocation {
                    message_index: new_index,
                    segment,
                });
            }
        }

        self.ensure_focus_visible();
    }

    fn focus_next_row(&mut self) {
        if self.transcriptions.is_empty() {
            return;
        }

        self.ensure_focus_valid();

        if let Some(current) = self.focus {
            let last_index = self.transcriptions.len() - 1;
            if current.message_index >= last_index {
                self.focus = Some(FocusLocation {
                    message_index: last_index,
                    segment: self.resolve_segment_for_message(last_index, current.segment),
                });
            } else {
                let new_index = current.message_index + 1;
                let segment = self.resolve_segment_for_message(new_index, current.segment);
                self.focus = Some(FocusLocation {
                    message_index: new_index,
                    segment,
                });
            }
        }

        self.ensure_focus_visible();
    }

    /// Clamp scroll offset so we never go beyond available history
    fn clamp_scroll(&mut self) {
        let total = self.transcriptions.len();
        if total == 0 {
            self.scroll_position = 0;
            return;
        }

        let visible_lines = if self.viewport_height == 0 {
            total
        } else {
            total.min(self.viewport_height)
        };

        let max_scroll = total.saturating_sub(visible_lines);
        if self.scroll_position > max_scroll {
            self.scroll_position = max_scroll;
        }
    }

    /// Ensure the focused item remains visible in the viewport
    fn ensure_focus_visible(&mut self) {
        if self.viewport_height == 0 {
            return;
        }

        self.ensure_focus_valid();
        self.clamp_scroll();

        let Some(focus) = self.focus else {
            return;
        };

        let total = self.transcriptions.len();
        if total == 0 {
            self.scroll_position = 0;
            return;
        }

        let visible_lines = total.min(self.viewport_height);
        let max_scroll = total.saturating_sub(visible_lines);
        let offset_from_bottom = self.scroll_position.min(max_scroll);
        let end_index = total.saturating_sub(offset_from_bottom);
        let start_index = end_index.saturating_sub(visible_lines);

        if focus.message_index < start_index {
            let desired_scroll = total.saturating_sub(focus.message_index + visible_lines);
            self.scroll_position = desired_scroll.min(max_scroll);
        } else if focus.message_index >= end_index {
            let desired_scroll = total.saturating_sub(focus.message_index + 1);
            self.scroll_position = desired_scroll.min(max_scroll);
        }
    }

    /// Keep focus aligned with existing messages
    fn ensure_focus_valid(&mut self) {
        if self.transcriptions.is_empty() {
            self.focus = None;
            self.scroll_position = 0;
            return;
        }

        let mut focus = self.focus.unwrap_or_else(|| FocusLocation {
            message_index: self.transcriptions.len() - 1,
            segment: FocusSegment::Message,
        });

        if focus.message_index >= self.transcriptions.len() {
            focus.message_index = self.transcriptions.len() - 1;
        }

        focus.segment = self.resolve_segment_for_message(focus.message_index, focus.segment);

        self.focus = Some(focus);
    }

    /// Adjust focus when the oldest message is removed
    fn handle_removed_front(&mut self) {
        if let Some(mut focus) = self.focus {
            if focus.message_index > 0 {
                focus.message_index -= 1;
            }

            if self.transcriptions.is_empty() {
                self.focus = None;
                self.scroll_position = 0;
                return;
            }

            if focus.message_index >= self.transcriptions.len() {
                focus.message_index = self.transcriptions.len() - 1;
            }

            focus.segment = self.resolve_segment_for_message(focus.message_index, focus.segment);
            self.focus = Some(focus);
        }
    }

    fn resolve_segment_for_message(&self, index: usize, preferred: FocusSegment) -> FocusSegment {
        if matches!(preferred, FocusSegment::Speaker) && self.message_has_speaker(index) {
            FocusSegment::Speaker
        } else {
            FocusSegment::Message
        }
    }

    fn message_has_speaker(&self, index: usize) -> bool {
        self.transcriptions
            .get(index)
            .and_then(|message| message.speaker.as_ref())
            .is_some()
    }
}

/// Transcription display widget
pub struct TranscriptionWidget;

impl TranscriptionWidget {
    /// Render the transcription widget
    pub fn render(
        frame: &mut Frame,
        state: &mut TranscriptionWidgetState,
        app_state: &AppState,
        area: Rect,
    ) {
        let content_height = area.height.saturating_sub(2).max(1) as usize;
        state.update_viewport_height(content_height);

        let total = state.transcriptions.len();

        let lines: Vec<Line> = if total == 0 {
            vec![Line::from(Span::styled(
                "Waiting for transcriptions...",
                Style::default().fg(Color::DarkGray),
            ))]
        } else {
            let visible_lines = total.min(content_height);
            let max_scroll = total.saturating_sub(visible_lines);
            let offset_from_bottom = state.scroll_position.min(max_scroll);
            let end_index = total.saturating_sub(offset_from_bottom);
            let start_index = end_index.saturating_sub(visible_lines);

            let focused = state.focus;
            let highlight_style = Style::default().bg(Color::Blue).fg(Color::White);
            let speaker_style = Style::default().fg(Color::LightCyan);
            let message_style = Style::default();

            let mut lines = Vec::with_capacity(visible_lines);
            for idx in start_index..end_index {
                if let Some(message) = state.transcriptions.get(idx) {
                    let mut spans: Vec<Span> = Vec::new();

                    if let Some(speaker) = &message.speaker {
                        let mut style = speaker_style;
                        if matches!(
                            focused,
                            Some(FocusLocation {
                                message_index,
                                segment: FocusSegment::Speaker,
                            }) if message_index == idx
                        ) {
                            style = highlight_style;
                        }
                        let speaker_text = format!("[{}]: ", speaker);
                        spans.push(Span::styled(speaker_text, style));
                    }

                    let mut style = message_style;
                    if matches!(
                        focused,
                        Some(FocusLocation {
                            message_index,
                            segment: FocusSegment::Message,
                        }) if message_index == idx
                    ) {
                        style = highlight_style;
                    }
                    spans.push(Span::styled(message.content.as_str(), style));

                    lines.push(Line::from(spans));
                }
            }

            lines
        };

        let paragraph = Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(build_title(app_state))
                    .title_top(build_device_title(app_state).right_aligned())
                    .border_type(BorderType::Rounded),
            )
            .wrap(Wrap { trim: false });

        frame.render_widget(paragraph, area);
    }
}

fn build_title(app_state: &AppState) -> Span<'_> {
    let (state_text, state_color) = match app_state.recording_state() {
        RecordingState::Recording => ("● RECORDING", Color::Red),
        RecordingState::Paused => ("⏸ PAUSED", Color::Yellow),
    };

    let timer_text = app_state.format_recording_time();
    Span::styled(
        format!(" Transcriptions {} {} ", state_text, timer_text),
        Style::default().fg(state_color).bold(),
    )
}

fn build_device_title(app_state: &AppState) -> Line<'_> {
    let device_name = get_device_name(app_state.current_device_index())
        .unwrap_or_else(|_| "Unknown Device".to_string());
    let device_title = format!(" 🎤 {} (D: Change) ", device_name);
    Line::from(Span::styled(device_title, Style::default().fg(Color::Cyan)))
}
