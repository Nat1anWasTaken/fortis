use ratatui::{
    prelude::*,
    widgets::*,
};

/// State for the device selection dialog
pub struct DeviceDialogState {
    /// Currently selected device index in the dialog
    pub selected_index: usize,
    /// List of available devices
    pub devices: Vec<String>,
    /// Index of the currently active device
    pub current_device_index: usize,
}

impl DeviceDialogState {
    pub fn new(devices: Vec<String>, current_device_index: usize) -> Self {
        Self {
            selected_index: current_device_index,
            devices,
            current_device_index,
        }
    }

    /// Move selection up
    pub fn select_previous(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    /// Move selection down
    pub fn select_next(&mut self) {
        if self.selected_index < self.devices.len().saturating_sub(1) {
            self.selected_index += 1;
        }
    }

    /// Get the currently selected device index
    pub fn selected(&self) -> usize {
        self.selected_index
    }
}

/// Device selection dialog widget
pub struct DeviceDialog;

impl StatefulWidget for DeviceDialog {
    type State = DeviceDialogState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        // Calculate centered dialog area
        let dialog_width = area.width.min(60);
        let dialog_height = (state.devices.len() as u16 + 4).min(area.height - 4);

        let horizontal_margin = (area.width.saturating_sub(dialog_width)) / 2;
        let vertical_margin = (area.height.saturating_sub(dialog_height)) / 2;

        let dialog_area = Rect {
            x: area.x + horizontal_margin,
            y: area.y + vertical_margin,
            width: dialog_width,
            height: dialog_height,
        };

        // Clear the area behind the dialog
        let clear_widget = Block::default()
            .style(Style::default().bg(Color::Black));
        clear_widget.render(dialog_area, buf);

        // Create the list items
        let items: Vec<ListItem> = state
            .devices
            .iter()
            .enumerate()
            .map(|(i, device)| {
                let prefix = if i == state.current_device_index {
                    "● "
                } else {
                    "  "
                };
                let content = format!("{}{}", prefix, device);
                let style = if i == state.selected_index {
                    Style::default().bg(Color::Blue).fg(Color::White)
                } else if i == state.current_device_index {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default()
                };
                ListItem::new(content).style(style)
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Select Audio Device ")
                    .title_alignment(Alignment::Left)
                    .border_type(BorderType::Rounded)
            );

        Widget::render(list, dialog_area, buf);
    }
}
