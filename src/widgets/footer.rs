use ratatui::{
    prelude::*,
    widgets::*,
};

/// Footer widget displaying control information
pub struct FooterWidget;

impl FooterWidget {
    /// Render the footer widget with control information
    pub fn render(frame: &mut Frame, area: Rect) {
        let controls = vec![
            ("SPACE", "Pause/Resume"),
            ("↑/↓", "Scroll"),
            ("q/ESC", "Quit"),
        ];

        let control_text: Vec<Span> = controls
            .iter()
            .flat_map(|(key, desc)| {
                vec![
                    Span::styled(*key, Style::default().fg(Color::Yellow).bold()),
                    Span::raw(": "),
                    Span::raw(*desc),
                    Span::raw("  "),
                ]
            })
            .collect();

        let paragraph = Paragraph::new(Line::from(control_text))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Controls ")
                    .border_type(BorderType::Rounded)
            )
            .alignment(Alignment::Center);

        frame.render_widget(paragraph, area);
    }
}
