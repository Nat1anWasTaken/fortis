use std::cmp::Ordering;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    prelude::*,
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::config::{ConfigEntry, ConfigField, ConfigGroup, ConfigManager, ConfigNode};

#[derive(Clone)]
enum DisplayItem {
    Group {
        depth: usize,
        label: String,
        description: Option<String>,
    },
    Entry {
        depth: usize,
        entry: ConfigEntry,
    },
}

impl DisplayItem {
    fn is_selectable(&self) -> bool {
        matches!(self, DisplayItem::Entry { .. })
    }
}

#[derive(Clone)]
struct TextEditState {
    key: String,
    buffer: String,
    cursor: usize,
    max_length: Option<usize>,
    secret: bool,
}

/// Captures the state of the settings dialog (selection, focus, etc.).
pub struct SettingsDialogState {
    items: Vec<DisplayItem>,
    selected_row: usize,
    editing: Option<TextEditState>,
}

impl SettingsDialogState {
    pub fn new(manager: &ConfigManager) -> Self {
        let mut items = Vec::new();
        build_items(manager.schema(), 0, false, &mut items);
        let selected_row = items
            .iter()
            .position(DisplayItem::is_selectable)
            .unwrap_or(0);

        Self {
            items,
            selected_row,
            editing: None,
        }
    }

    pub fn selected_row(&self) -> usize {
        self.selected_row
    }

    pub fn selected_entry(&self) -> Option<&ConfigEntry> {
        match self.items.get(self.selected_row) {
            Some(DisplayItem::Entry { entry, .. }) => Some(entry),
            _ => None,
        }
    }

    fn move_selection(&mut self, direction: Ordering) -> bool {
        if self.items.is_empty() {
            return false;
        }

        let mut index = self.selected_row;
        loop {
            match direction {
                Ordering::Less => {
                    if index == 0 {
                        return false;
                    }
                    index -= 1;
                }
                Ordering::Greater => {
                    index += 1;
                    if index >= self.items.len() {
                        return false;
                    }
                }
                Ordering::Equal => return false,
            }

            if self.items[index].is_selectable() {
                self.selected_row = index;
                self.editing = None;
                return true;
            }

            if (direction == Ordering::Less && index == 0)
                || (direction == Ordering::Greater && index >= self.items.len() - 1)
            {
                return false;
            }
        }
    }

    pub fn select_previous(&mut self) -> bool {
        self.move_selection(Ordering::Less)
    }

    pub fn select_next(&mut self) -> bool {
        self.move_selection(Ordering::Greater)
    }

    fn items(&self) -> &[DisplayItem] {
        &self.items
    }
}

/// Result of handling a key input within the dialog.
pub struct DialogEvent {
    pub handled: bool,
    pub close: bool,
    pub value_changed: bool,
}

impl DialogEvent {
    fn unhandled() -> Self {
        Self {
            handled: false,
            close: false,
            value_changed: false,
        }
    }
}

impl SettingsDialogState {
    pub fn handle_key_event(&mut self, key: KeyEvent, manager: &mut ConfigManager) -> DialogEvent {
        let mut event = DialogEvent::unhandled();

        if let Some(edit_state) = self.editing.as_mut() {
            event.handled = true;
            match key.code {
                KeyCode::Esc => {
                    self.editing = None;
                }
                KeyCode::Enter => {
                    let buffer = edit_state.buffer.clone();
                    match manager.set_text(&edit_state.key, &buffer) {
                        Ok(changed) => event.value_changed |= changed,
                        Err(err) => eprintln!("Failed to update {}: {err}", edit_state.key),
                    }
                    self.editing = None;
                }
                KeyCode::Backspace => {
                    if edit_state.cursor > 0 {
                        if let Some((idx, _)) = edit_state
                            .buffer
                            .char_indices()
                            .take_while(|(byte_idx, _)| *byte_idx < edit_state.cursor)
                            .last()
                        {
                            edit_state.buffer.drain(idx..edit_state.cursor);
                            edit_state.cursor = idx;
                        }
                    }
                }
                KeyCode::Delete => {
                    if edit_state.cursor < edit_state.buffer.len() {
                        if let Some((offset, ch)) =
                            edit_state.buffer[edit_state.cursor..].char_indices().next()
                        {
                            let start = edit_state.cursor + offset;
                            let end = start + ch.len_utf8();
                            edit_state.buffer.drain(start..end);
                        }
                    }
                }
                KeyCode::Left => {
                    edit_state.cursor =
                        previous_char_boundary(&edit_state.buffer, edit_state.cursor);
                }
                KeyCode::Right => {
                    edit_state.cursor = next_char_boundary(&edit_state.buffer, edit_state.cursor);
                }
                KeyCode::Home => {
                    edit_state.cursor = 0;
                }
                KeyCode::End => {
                    edit_state.cursor = edit_state.buffer.len();
                }
                KeyCode::Char(c) => {
                    if !c.is_control()
                        && edit_state
                            .max_length
                            .map_or(true, |max| edit_state.buffer.chars().count() < max)
                    {
                        edit_state.buffer.insert(edit_state.cursor, c);
                        edit_state.cursor += c.len_utf8();
                    }
                }
                KeyCode::Tab | KeyCode::BackTab => {
                    // ignore navigation keys while editing text
                }
                _ => {}
            }
            return event;
        }

        match key.code {
            KeyCode::Esc => {
                event.handled = true;
                event.close = true;
                return event;
            }
            KeyCode::Up => {
                event.handled = true;
                self.select_previous();
                return event;
            }
            KeyCode::Down => {
                event.handled = true;
                self.select_next();
                return event;
            }
            KeyCode::Tab => {
                event.handled = true;
                self.select_next();
                return event;
            }
            KeyCode::BackTab => {
                event.handled = true;
                self.select_previous();
                return event;
            }
            _ => {}
        }

        let Some(entry) = self.selected_entry().cloned() else {
            return event;
        };

        match (&entry.field, key.code) {
            (ConfigField::Toggle { .. }, KeyCode::Char(' '))
            | (ConfigField::Toggle { .. }, KeyCode::Enter) => {
                event.handled = true;
                match manager.toggle_bool(&entry.key) {
                    Ok(changed) => event.value_changed |= changed,
                    Err(err) => eprintln!("Failed to update setting {}: {err}", entry.key),
                }
            }
            (ConfigField::Number(_), KeyCode::Left) => {
                event.handled = true;
                let steps = step_multiplier(key.modifiers);
                match manager.adjust_number(&entry.key, -steps) {
                    Ok(changed) => event.value_changed |= changed,
                    Err(err) => eprintln!("Failed to adjust {}: {err}", entry.key),
                }
            }
            (ConfigField::Number(_), KeyCode::Right) => {
                event.handled = true;
                let steps = step_multiplier(key.modifiers);
                match manager.adjust_number(&entry.key, steps) {
                    Ok(changed) => event.value_changed |= changed,
                    Err(err) => eprintln!("Failed to adjust {}: {err}", entry.key),
                }
            }
            (ConfigField::Number(_), KeyCode::Char('-'))
            | (ConfigField::Number(_), KeyCode::Char('_')) => {
                event.handled = true;
                match manager.adjust_number(&entry.key, -1.0) {
                    Ok(changed) => event.value_changed |= changed,
                    Err(err) => eprintln!("Failed to adjust {}: {err}", entry.key),
                }
            }
            (ConfigField::Number(_), KeyCode::Char('+'))
            | (ConfigField::Number(_), KeyCode::Char('=')) => {
                event.handled = true;
                match manager.adjust_number(&entry.key, 1.0) {
                    Ok(changed) => event.value_changed |= changed,
                    Err(err) => eprintln!("Failed to adjust {}: {err}", entry.key),
                }
            }
            (ConfigField::Select { .. }, KeyCode::Left) => {
                event.handled = true;
                match manager.cycle_select(&entry.key, -1) {
                    Ok(changed) => event.value_changed |= changed,
                    Err(err) => eprintln!("Failed to update {}: {err}", entry.key),
                }
            }
            (ConfigField::Select { .. }, KeyCode::Right)
            | (ConfigField::Select { .. }, KeyCode::Enter)
            | (ConfigField::Select { .. }, KeyCode::Char(' ')) => {
                event.handled = true;
                match manager.cycle_select(&entry.key, 1) {
                    Ok(changed) => event.value_changed |= changed,
                    Err(err) => eprintln!("Failed to update {}: {err}", entry.key),
                }
            }
            (ConfigField::Text(_), KeyCode::Enter) => {
                event.handled = true;
                self.begin_text_edit(&entry, &*manager, None);
            }
            (ConfigField::Text(_), KeyCode::Char(c)) => {
                event.handled = true;
                self.begin_text_edit(&entry, &*manager, Some(c));
            }
            _ => {}
        }

        event
    }

    fn begin_text_edit(
        &mut self,
        entry: &ConfigEntry,
        manager: &ConfigManager,
        initial_char: Option<char>,
    ) {
        let ConfigField::Text(field) = &entry.field else {
            return;
        };

        let mut buffer = manager
            .text_value(&entry.key)
            .unwrap_or_else(|_| field.default.clone());
        let mut cursor = buffer.len();

        if let Some(ch) = initial_char {
            if !ch.is_control()
                && field
                    .max_length
                    .map_or(true, |max| buffer.chars().count() < max)
            {
                buffer.push(ch);
                cursor = buffer.len();
            }
        }

        self.editing = Some(TextEditState {
            key: entry.key.clone(),
            buffer,
            cursor,
            max_length: field.max_length,
            secret: field.secret,
        });
    }
}

/// Settings dialog widget capable of rendering dynamic configuration entries.
pub struct SettingsDialog<'a> {
    pub manager: &'a ConfigManager,
    pub accent: Color,
}

impl<'a> StatefulWidget for SettingsDialog<'a> {
    type State = SettingsDialogState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let mut width = area.width.min(80);
        width = width.max(40);
        width = width.min(area.width);

        let mut height = area.height.min(28);
        height = height.max(12);
        height = height.min(area.height);

        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;

        let dialog_area = Rect {
            x,
            y,
            width,
            height,
        };

        Clear.render(dialog_area, buf);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Settings ")
            .border_type(BorderType::Rounded)
            .style(Style::default().bg(Color::Black));
        block.render(dialog_area, buf);

        if dialog_area.width <= 2 || dialog_area.height <= 2 {
            return;
        }

        let inner = Rect {
            x: dialog_area.x + 1,
            y: dialog_area.y + 1,
            width: dialog_area.width - 2,
            height: dialog_area.height - 2,
        };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(4), Constraint::Length(4)])
            .split(inner);

        let list_area = chunks[0];
        let detail_area = chunks[1];

        let available_width = list_area.width.saturating_sub(4) as usize;
        let mut items: Vec<ListItem> = Vec::with_capacity(state.items().len());

        for item in state.items() {
            match item {
                DisplayItem::Group {
                    depth,
                    label,
                    description: _,
                } => {
                    let indent = "  ".repeat(*depth);
                    let text = format!("{indent}{}", label);
                    let line = Line::from(Span::styled(
                        text,
                        Style::default()
                            .fg(self.accent)
                            .add_modifier(Modifier::BOLD),
                    ));
                    items.push(ListItem::new(line));
                }
                DisplayItem::Entry { depth, entry } => {
                    let indent = "  ".repeat(*depth);
                    let label_text = format!("{indent}{}", entry.label);
                    let label_width = label_text.chars().count();
                    let editing_state = state.editing.as_ref().filter(|edit| edit.key == entry.key);
                    let (value_spans, value_width) =
                        value_spans_for_entry(entry, self.manager, editing_state, self.accent);
                    let padding = available_width
                        .saturating_sub(label_width + value_width)
                        .min(available_width);
                    let mut spans = Vec::new();
                    spans.push(Span::raw(label_text));
                    spans.push(Span::raw(" ".repeat(padding)));
                    spans.extend(value_spans);
                    let line = Line::from(spans);
                    items.push(ListItem::new(line));
                }
            }
        }

        let mut list_state = ListState::default();
        if !state.items().is_empty() {
            list_state.select(Some(state.selected_row()));
        }

        let list = List::new(items)
            .highlight_style(
                Style::default()
                    .fg(Color::Black)
                    .bg(self.accent)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("› ")
            .block(Block::default().style(Style::default().bg(Color::Black)));
        ratatui::widgets::StatefulWidget::render(list, list_area, buf, &mut list_state);

        render_detail_panel(detail_area, buf, state, self.manager, self.accent);
    }
}

fn render_detail_panel(
    area: Rect,
    buf: &mut Buffer,
    state: &SettingsDialogState,
    manager: &ConfigManager,
    accent: Color,
) {
    let mut lines = Vec::new();

    if let Some(item) = state.items().get(state.selected_row()) {
        match item {
            DisplayItem::Group { description, .. } => {
                if let Some(desc) = description {
                    lines.push(Line::from(Span::raw(desc.clone())));
                }
            }
            DisplayItem::Entry { entry, .. } => {
                if let Some(desc) = &entry.description {
                    lines.push(Line::from(Span::raw(desc.clone())));
                }

                let instructions = match &entry.field {
                    ConfigField::Toggle { .. } => "SPACE/ENTER toggle • ↑/↓ navigate • ESC close",
                    ConfigField::Number(_) => {
                        "←/→ adjust • +/- fine tune • Shift/Ctrl for larger steps • ESC close"
                    }
                    ConfigField::Select { .. } => {
                        "←/→ cycle options • SPACE/ENTER advance • ESC close"
                    }
                    ConfigField::Text(_) => {
                        "ENTER edit • type to change • ENTER saves • ESC cancels"
                    }
                };
                lines.push(Line::from(Span::styled(
                    instructions,
                    Style::default().fg(accent),
                )));

                let value_preview = match &entry.field {
                    ConfigField::Text(field) => match manager.text_value(&entry.key) {
                        Ok(value) if value.is_empty() => "Current value: (not set)".to_string(),
                        Ok(_) if field.secret => "Current value: (hidden)".to_string(),
                        Ok(value) => format!("Current value: {value}"),
                        Err(_) => String::new(),
                    },
                    _ => format!("Current value: {}", format_value(entry, manager)),
                };
                if !value_preview.is_empty() {
                    lines.push(Line::from(Span::raw(value_preview)));
                }
            }
        }
    }

    if lines.is_empty() {
        lines.push(Line::from(Span::raw(
            "↑/↓ to move • SPACE toggles • ESC closes settings",
        )));
    }

    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: true }).block(
        Block::default()
            .borders(Borders::TOP)
            .border_type(BorderType::Plain)
            .style(Style::default().bg(Color::Black)),
    );

    Paragraph::render(paragraph, area, buf);
}

fn format_value(entry: &ConfigEntry, manager: &ConfigManager) -> String {
    match &entry.field {
        ConfigField::Toggle { .. } => {
            let enabled = manager.bool_value(&entry.key).unwrap_or(false);
            if enabled {
                "On".to_string()
            } else {
                "Off".to_string()
            }
        }
        ConfigField::Number(field) => {
            let value = manager.number_value(&entry.key).unwrap_or(field.default);
            let precision = field.precision.unwrap_or(0) as usize;
            let formatted = if precision == 0 {
                format!("{value:.0}")
            } else {
                let raw = format!("{value:.prec$}", value = value, prec = precision);
                let trimmed = raw.trim_end_matches('0').trim_end_matches('.');
                if trimmed.is_empty() {
                    raw
                } else {
                    trimmed.to_string()
                }
            };
            if let Some(unit) = &field.unit {
                format!("{formatted} {unit}")
            } else {
                formatted
            }
        }
        ConfigField::Select { options, .. } => {
            if let Ok(current) = manager.select_value(&entry.key) {
                if let Some(option) = options.iter().find(|opt| opt.value == current) {
                    option.label.clone()
                } else {
                    current
                }
            } else {
                String::from("—")
            }
        }
        ConfigField::Text(field) => manager
            .text_value(&entry.key)
            .unwrap_or_else(|_| field.default.clone()),
    }
}

fn value_spans_for_entry(
    entry: &ConfigEntry,
    manager: &ConfigManager,
    editing: Option<&TextEditState>,
    accent: Color,
) -> (Vec<Span<'static>>, usize) {
    match (&entry.field, editing) {
        (ConfigField::Text(_), Some(edit_state)) => {
            let mut spans = Vec::new();
            let before = edit_state.buffer[..edit_state.cursor].to_string();
            let after = edit_state.buffer[edit_state.cursor..].to_string();
            let text_style = if edit_state.secret {
                Style::default().fg(accent)
            } else {
                Style::default().fg(accent).add_modifier(Modifier::BOLD)
            };

            let width = before.chars().count() + after.chars().count() + 1;

            if !before.is_empty() {
                spans.push(Span::styled(before.clone(), text_style));
            }

            spans.push(Span::styled(
                "█",
                Style::default()
                    .fg(Color::Black)
                    .bg(accent)
                    .add_modifier(Modifier::BOLD),
            ));

            if !after.is_empty() {
                spans.push(Span::styled(after.clone(), text_style));
            }

            (spans, width)
        }
        (ConfigField::Text(field), None) => {
            let value = manager
                .text_value(&entry.key)
                .unwrap_or_else(|_| field.default.clone());
            if value.is_empty() {
                let placeholder = field
                    .placeholder
                    .clone()
                    .unwrap_or_else(|| "<not set>".to_string());
                (
                    vec![Span::styled(
                        placeholder.clone(),
                        Style::default()
                            .fg(Color::DarkGray)
                            .add_modifier(Modifier::ITALIC),
                    )],
                    placeholder.chars().count(),
                )
            } else if field.secret {
                let mask_len = value.chars().count().clamp(4, 12);
                let masked = "*".repeat(mask_len);
                (
                    vec![Span::styled(
                        masked,
                        Style::default().fg(accent).add_modifier(Modifier::BOLD),
                    )],
                    mask_len,
                )
            } else {
                let display = value.clone();
                let width = display.chars().count();
                (
                    vec![Span::styled(
                        display,
                        Style::default().fg(accent).add_modifier(Modifier::BOLD),
                    )],
                    width,
                )
            }
        }
        _ => {
            let value_text = format_value(entry, manager);
            let width = value_text.chars().count();
            (
                vec![Span::styled(
                    value_text,
                    Style::default().fg(accent).add_modifier(Modifier::BOLD),
                )],
                width,
            )
        }
    }
}

fn build_items(
    group: &ConfigGroup,
    depth: usize,
    include_self: bool,
    items: &mut Vec<DisplayItem>,
) {
    if include_self {
        items.push(DisplayItem::Group {
            depth,
            label: group.label.clone(),
            description: group.description.clone(),
        });
    }

    for node in &group.children {
        match node {
            ConfigNode::Group(child) => {
                let next_depth = if include_self { depth + 1 } else { depth };
                build_items(child, next_depth, true, items);
            }
            ConfigNode::Entry(entry) => {
                let entry_depth = if include_self { depth + 1 } else { depth };
                items.push(DisplayItem::Entry {
                    depth: entry_depth,
                    entry: entry.clone(),
                });
            }
        }
    }
}

fn previous_char_boundary(text: &str, cursor: usize) -> usize {
    if cursor == 0 {
        0
    } else {
        text[..cursor]
            .char_indices()
            .next_back()
            .map(|(idx, _)| idx)
            .unwrap_or(0)
    }
}

fn next_char_boundary(text: &str, cursor: usize) -> usize {
    if cursor >= text.len() {
        text.len()
    } else {
        text[cursor..]
            .char_indices()
            .next()
            .map(|(offset, ch)| cursor + offset + ch.len_utf8())
            .unwrap_or(text.len())
    }
}

fn step_multiplier(modifiers: KeyModifiers) -> f64 {
    if modifiers.contains(KeyModifiers::CONTROL) {
        10.0
    } else if modifiers.contains(KeyModifiers::SHIFT) {
        5.0
    } else {
        1.0
    }
}
