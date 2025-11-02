use std::io::{self, stdout};
use std::time::Duration;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{
    prelude::*,
    widgets::*,
};

/// Application state for the TUI
pub struct App {
    /// List of transcription messages
    pub transcriptions: Vec<String>,
    /// Whether the app should quit
    pub should_quit: bool,
    /// Current scroll position
    pub scroll_position: usize,
}

impl App {
    pub fn new() -> Self {
        Self {
            transcriptions: Vec::new(),
            should_quit: false,
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

    /// Handle keyboard input
    pub fn handle_key_event(&mut self, key: event::KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.should_quit = true;
            }
            KeyCode::Up => {
                self.scroll_up();
            }
            KeyCode::Down => {
                self.scroll_down();
            }
            _ => {}
        }
    }
}

/// Initialize the terminal for TUI mode
pub fn init_terminal() -> io::Result<Terminal<CrosstermBackend<std::io::Stdout>>> {
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout());
    Terminal::new(backend)
}

/// Restore the terminal to normal mode
pub fn restore_terminal() -> io::Result<()> {
    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}

/// Render the UI
pub fn render_ui(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),      // Main content
            Constraint::Length(3),    // Footer
        ])
        .split(frame.area());

    // Render transcriptions block
    render_transcriptions(frame, app, chunks[0]);

    // Render footer with controls
    render_footer(frame, chunks[1]);
}

/// Render the transcriptions block
fn render_transcriptions(frame: &mut Frame, app: &App, area: Rect) {
    let text: Vec<Line> = if app.transcriptions.is_empty() {
        vec![Line::from(Span::styled(
            "Waiting for transcriptions...",
            Style::default().fg(Color::DarkGray),
        ))]
    } else {
        app.transcriptions
            .iter()
            .map(|msg| Line::from(msg.clone()))
            .collect()
    };

    let paragraph = Paragraph::new(text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Transcriptions ")
                .title_alignment(Alignment::Center)
                .border_type(BorderType::Rounded)
        )
        .wrap(Wrap { trim: false })
        .scroll((app.scroll_position as u16, 0));

    frame.render_widget(paragraph, area);
}

/// Render the footer with control information
fn render_footer(frame: &mut Frame, area: Rect) {
    let controls = vec![
        ("q/ESC", "Quit"),
        ("↑/↓", "Scroll"),
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

/// Poll for keyboard events with timeout
pub fn poll_events(timeout: Duration) -> io::Result<Option<Event>> {
    if event::poll(timeout)? {
        Ok(Some(event::read()?))
    } else {
        Ok(None)
    }
}
