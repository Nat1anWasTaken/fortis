use std::error::Error;
use std::io::Stdout;
use crossterm::{
    event::{self, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Terminal,
};

use crate::state::{AppModel, AppState};
use crate::event_handler::{handle_key_event, get_help_text, get_status_message, AppEvent};

pub struct TerminalUi {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalUi {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        enable_raw_mode()?;
        let mut stdout = std::io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;

        Ok(Self { terminal })
    }

    /// Draw the current frame with the app model
    pub fn draw(&mut self, model: &AppModel) -> Result<(), Box<dyn Error>> {
        self.terminal.draw(|f| render_ui(f, model))?;
        Ok(())
    }

    pub fn check_key_press(&mut self) -> Result<Option<AppEvent>, Box<dyn Error>> {
        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                return Ok(handle_key_event(key));
            }
        }
        Ok(None)
    }
}

impl Drop for TerminalUi {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

fn render_ui(f: &mut ratatui::Frame, model: &AppModel) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(0)
        .constraints(
            [
                Constraint::Length(3),
                Constraint::Min(5),
                Constraint::Length(3),
            ]
            .as_ref(),
        )
        .split(f.size());

    render_header(f, model, chunks[0]);
    render_content(f, model, chunks[1]);
    render_footer(f, model, chunks[2]);
}

fn render_header(f: &mut ratatui::Frame, model: &AppModel, area: ratatui::layout::Rect) {
    let (status_color, state_text) = match model.state {
        AppState::SelectingDevice => (Color::Yellow, "SELECTING DEVICE"),
        AppState::ReadyToRecord => (Color::Magenta, "READY TO RECORD"),
        AppState::Recording => (Color::Green, "RECORDING"),
        AppState::Transcribing => (Color::Blue, "TRANSCRIBING"),
        AppState::Done => (Color::Cyan, "DONE"),
    };

    let title = Line::from(vec![
        Span::styled("fortis", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw(" - Audio Transcription"),
    ]);

    let status = Line::from(vec![
        Span::raw("Status: "),
        Span::styled(state_text, Style::default().fg(status_color).add_modifier(Modifier::BOLD)),
    ]);

    let header = Paragraph::new(vec![title, status])
        .block(Block::default().borders(Borders::BOTTOM).style(Style::default().fg(Color::Cyan)));

    f.render_widget(header, area);
}

fn render_content(f: &mut ratatui::Frame, model: &AppModel, area: ratatui::layout::Rect) {
    match model.state {
        AppState::SelectingDevice => render_device_selection(f, model, area),
        _ => render_transcript_area(f, model, area),
    }
}

fn render_device_selection(f: &mut ratatui::Frame, model: &AppModel, area: ratatui::layout::Rect) {
    let device_block = Block::default()
        .title("Select Audio Device")
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Cyan));

    let mut device_lines = Vec::new();
    for (index, device) in model.devices.iter().enumerate() {
        let prefix = if index == model.selected_device {
            "▶ "
        } else {
            "  "
        };

        let style = if index == model.selected_device {
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };

        device_lines.push(Line::from(Span::styled(
            format!("{}{}", prefix, device),
            style,
        )));
    }

    let devices_para = Paragraph::new(device_lines).block(device_block);
    f.render_widget(devices_para, area);
}

fn render_transcript_area(f: &mut ratatui::Frame, model: &AppModel, area: ratatui::layout::Rect) {
    let transcript_block = Block::default()
        .title("Transcript (Page Up/Down to scroll)")
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Cyan));

    if model.transcript_lines.is_empty() {
        let waiting = Paragraph::new("Waiting for transcription...")
            .style(Style::default().fg(Color::DarkGray))
            .block(transcript_block);
        f.render_widget(waiting, area);
    } else {
        let visible_lines: Vec<Line> = model
            .transcript_lines
            .iter()
            .skip(model.scroll_position)
            .take(area.height.saturating_sub(2) as usize)
            .map(|line| Line::from(line.as_str()))
            .collect();

        let transcript = Paragraph::new(visible_lines)
            .style(Style::default().fg(Color::White))
            .block(transcript_block);
        f.render_widget(transcript, area);
    }
}

fn render_footer(f: &mut ratatui::Frame, model: &AppModel, area: ratatui::layout::Rect) {
    let footer = Paragraph::new(vec![
        Line::from(get_status_message(model)),
        Line::from(get_help_text(model.state)),
    ])
    .block(Block::default().borders(Borders::TOP).style(Style::default().fg(Color::DarkGray)));

    f.render_widget(footer, area);
}
