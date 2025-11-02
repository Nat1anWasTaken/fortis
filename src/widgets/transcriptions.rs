use std::collections::VecDeque;

use ratatui::{prelude::*, widgets::*};

use crate::state::{AppState, RecordingState};

#[derive(Debug, Clone)]
pub struct TranscriptionMessage {
    pub speaker: Option<String>,
    pub speaker_id: Option<i32>,
    pub content: String,
}

impl TranscriptionMessage {
    pub fn new(speaker: Option<String>, speaker_id: Option<i32>, content: String) -> Self {
        Self {
            speaker,
            speaker_id,
            content,
        }
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

/// Edit mode state for modifying speaker names or message content
#[derive(Debug, Clone, PartialEq, Eq)]
enum EditMode {
    None,
    EditingSpeaker {
        message_index: usize,
        buffer: String,
        cursor: usize,
    },
    EditingMessage {
        message_index: usize,
        buffer: String,
        cursor: usize,
    },
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
    /// Current edit mode state
    edit_mode: EditMode,
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
            edit_mode: EditMode::None,
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

    /// Check if currently in edit mode
    pub fn is_editing(&self) -> bool {
        !matches!(self.edit_mode, EditMode::None)
    }

    /// Start editing the currently focused segment
    pub fn start_editing(&mut self) {
        if self.is_editing() {
            return;
        }

        self.ensure_focus_valid();
        let Some(focus) = self.focus else {
            return;
        };

        let Some(message) = self.transcriptions.get(focus.message_index) else {
            return;
        };

        match focus.segment {
            FocusSegment::Speaker => {
                if let Some(speaker) = &message.speaker {
                    self.edit_mode = EditMode::EditingSpeaker {
                        message_index: focus.message_index,
                        buffer: speaker.clone(),
                        cursor: speaker.len(),
                    };
                }
            }
            FocusSegment::Message => {
                self.edit_mode = EditMode::EditingMessage {
                    message_index: focus.message_index,
                    buffer: message.content.clone(),
                    cursor: message.content.len(),
                };
            }
        }
    }

    /// Cancel editing and discard changes
    pub fn cancel_editing(&mut self) {
        self.edit_mode = EditMode::None;
    }

    /// Apply the current edit and update the message
    pub fn apply_edit(&mut self, app_state: &mut AppState) {
        match &self.edit_mode {
            EditMode::EditingSpeaker {
                message_index,
                buffer,
                ..
            } => {
                if let Some(message) = self.transcriptions.get(*message_index) {
                    let trimmed = buffer.trim().to_string();
                    if !trimmed.is_empty() {
                        // Update the speaker mapping if we have a speaker_id
                        if let Some(speaker_id) = message.speaker_id {
                            app_state.set_speaker_name(speaker_id, trimmed.clone());

                            // Update all messages with the same speaker_id
                            for msg in self.transcriptions.iter_mut() {
                                if msg.speaker_id == Some(speaker_id) {
                                    msg.speaker = Some(trimmed.clone());
                                }
                            }
                        } else {
                            // No speaker_id, just update this message
                            if let Some(msg) = self.transcriptions.get_mut(*message_index) {
                                msg.speaker = Some(trimmed);
                            }
                        }
                    }
                }
            }
            EditMode::EditingMessage {
                message_index,
                buffer,
                ..
            } => {
                if let Some(message) = self.transcriptions.get_mut(*message_index) {
                    message.content = buffer.clone();
                }
            }
            EditMode::None => {}
        }
        self.edit_mode = EditMode::None;
    }

    /// Handle a character input during editing
    pub fn handle_char_input(&mut self, c: char) {
        match &mut self.edit_mode {
            EditMode::EditingSpeaker {
                buffer, cursor, ..
            } => {
                buffer.insert(*cursor, c);
                *cursor += 1;
            }
            EditMode::EditingMessage {
                buffer, cursor, ..
            } => {
                buffer.insert(*cursor, c);
                *cursor += 1;
            }
            EditMode::None => {}
        }
    }

    /// Handle backspace during editing
    pub fn handle_backspace(&mut self) {
        match &mut self.edit_mode {
            EditMode::EditingSpeaker {
                buffer, cursor, ..
            } => {
                if *cursor > 0 {
                    *cursor -= 1;
                    buffer.remove(*cursor);
                }
            }
            EditMode::EditingMessage {
                buffer, cursor, ..
            } => {
                if *cursor > 0 {
                    *cursor -= 1;
                    buffer.remove(*cursor);
                }
            }
            EditMode::None => {}
        }
    }

    /// Move cursor left during editing
    pub fn move_cursor_left(&mut self) {
        match &mut self.edit_mode {
            EditMode::EditingSpeaker { cursor, .. } | EditMode::EditingMessage { cursor, .. } => {
                if *cursor > 0 {
                    *cursor -= 1;
                }
            }
            EditMode::None => {}
        }
    }

    /// Move cursor right during editing
    pub fn move_cursor_right(&mut self) {
        match &mut self.edit_mode {
            EditMode::EditingSpeaker {
                buffer, cursor, ..
            } => {
                if *cursor < buffer.len() {
                    *cursor += 1;
                }
            }
            EditMode::EditingMessage {
                buffer, cursor, ..
            } => {
                if *cursor < buffer.len() {
                    *cursor += 1;
                }
            }
            EditMode::None => {}
        }
    }

    /// Get the current edit buffer and cursor for rendering
    pub fn get_edit_state(&self) -> Option<(&str, usize, bool)> {
        match &self.edit_mode {
            EditMode::EditingSpeaker {
                buffer, cursor, ..
            } => Some((buffer, *cursor, true)),
            EditMode::EditingMessage {
                buffer, cursor, ..
            } => Some((buffer, *cursor, false)),
            EditMode::None => None,
        }
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
            let highlight_style = Style::default().fg(Color::Yellow);
            let edit_style = Style::default().fg(Color::Green);
            let speaker_style = Style::default().fg(Color::LightCyan);
            let message_style = Style::default();

            let edit_state = state.get_edit_state();

            let mut lines = Vec::with_capacity(visible_lines);
            for idx in start_index..end_index {
                if let Some(message) = state.transcriptions.get(idx) {
                    let mut spans: Vec<Span> = Vec::new();

                    // Render speaker segment
                    if let Some(speaker) = &message.speaker {
                        let is_focused = matches!(
                            focused,
                            Some(FocusLocation {
                                message_index,
                                segment: FocusSegment::Speaker,
                            }) if message_index == idx
                        );

                        // Check if this segment is being edited
                        if let Some((buffer, cursor, is_speaker_edit)) = edit_state {
                            if is_speaker_edit
                                && matches!(
                                    &state.edit_mode,
                                    EditMode::EditingSpeaker { message_index, .. } if *message_index == idx
                                )
                            {
                                // Render with cursor
                                spans.push(Span::raw("["));
                                let before = &buffer[..cursor];
                                let after = &buffer[cursor..];
                                if !before.is_empty() {
                                    spans.push(Span::styled(before, edit_style));
                                }
                                spans.push(Span::styled("█", edit_style.add_modifier(Modifier::REVERSED)));
                                if !after.is_empty() {
                                    spans.push(Span::styled(after, edit_style));
                                }
                                spans.push(Span::raw("]: "));
                            } else {
                                // Normal display with focus highlight
                                let style = if is_focused { highlight_style } else { speaker_style };
                                let speaker_text = format!("[{}]: ", speaker);
                                spans.push(Span::styled(speaker_text, style));
                            }
                        } else {
                            // Normal display with focus highlight
                            let style = if is_focused { highlight_style } else { speaker_style };
                            let speaker_text = format!("[{}]: ", speaker);
                            spans.push(Span::styled(speaker_text, style));
                        }
                    }

                    // Render message segment
                    let is_focused = matches!(
                        focused,
                        Some(FocusLocation {
                            message_index,
                            segment: FocusSegment::Message,
                        }) if message_index == idx
                    );

                    // Check if this segment is being edited
                    if let Some((buffer, cursor, is_speaker_edit)) = edit_state {
                        if !is_speaker_edit
                            && matches!(
                                &state.edit_mode,
                                EditMode::EditingMessage { message_index, .. } if *message_index == idx
                            )
                        {
                            // Render with cursor
                            let before = &buffer[..cursor];
                            let after = &buffer[cursor..];
                            if !before.is_empty() {
                                spans.push(Span::styled(before, edit_style));
                            }
                            spans.push(Span::styled("█", edit_style.add_modifier(Modifier::REVERSED)));
                            if !after.is_empty() {
                                spans.push(Span::styled(after, edit_style));
                            }
                        } else {
                            // Normal display with focus highlight
                            let style = if is_focused { highlight_style } else { message_style };
                            spans.push(Span::styled(message.content.as_str(), style));
                        }
                    } else {
                        // Normal display with focus highlight
                        let style = if is_focused { highlight_style } else { message_style };
                        spans.push(Span::styled(message.content.as_str(), style));
                    }

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
            );
            // Note: Wrapping disabled for performance (was taking 80ms+)

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
    // Use cached device name to avoid expensive system calls every frame
    let device_name = app_state.current_device_name();
    let device_title = format!(" 🎤 {} (D: Change) ", device_name);
    Line::from(Span::styled(device_title, Style::default().fg(Color::Cyan)))
}
