use ratatui::{prelude::*, widgets::*};

/// Footer widget displaying control information
pub struct FooterWidget;

impl FooterWidget {
    /// Render the footer widget with control information
    pub fn render(frame: &mut Frame, area: Rect, accent: Color, compact: bool) {
        let controls = vec![
            ("SPACE", "Pause/Resume"),
            ("↑/↓", "Scroll"),
            ("←/→", "Focus Speaker/Message"),
            ("ENTER", "Edit"),
            ("S", "Settings"),
            ("q/ESC", "Quit"),
        ];

        let separator = if compact { " " } else { "   " };

        let mut spans: Vec<Span> = Vec::new();
        for (idx, (key, desc)) in controls.iter().enumerate() {
            if idx > 0 {
                spans.push(Span::raw(separator));
            }
            spans.push(Span::styled(
                *key,
                Style::default().fg(accent).add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::raw(": "));
            spans.push(Span::raw(*desc));
        }

        let paragraph = Paragraph::new(Line::from(spans))
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title(" Controls "),
            );

        frame.render_widget(paragraph, area);
    }
}
