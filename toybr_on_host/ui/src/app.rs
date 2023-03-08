use alloc::rc::Rc;
use alloc::string::ToString;
use core::cell::RefCell;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};
use renderer::frame::frame::Frame as RenderFrame;
use renderer::ui::UiObject;
use renderer::utils::*;
use std::{error::Error, io};
use tui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Span, Spans, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame, Terminal,
};
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Copy)]
enum InputMode {
    Normal,
    Editing,
}

enum LogLevel {
    Debug,
    Error,
}

struct Log {
    level: LogLevel,
    log: String,
}

impl Log {
    fn new(level: LogLevel, log: String) -> Self {
        Self { level, log }
    }
}

impl ToString for Log {
    fn to_string(&self) -> String {
        match self.level {
            LogLevel::Debug => format!("[DEBUG] {}", self.log),
            LogLevel::Error => format!("[ERROR] {}", self.log),
        }
    }
}

pub struct TuiBrowser {
    input_url: String,
    input_mode: InputMode,
    contents: Vec<String>,
    logs: Vec<Log>,
    render_frame: Option<RenderFrame<Self>>,
}

impl UiObject for TuiBrowser {
    fn new() -> Self {
        Self {
            input_url: String::new(),
            input_mode: InputMode::Normal,
            contents: Vec::new(),
            logs: Vec::new(),
            render_frame: None,
        }
    }

    fn println(&mut self, text: String) {
        self.contents.push(text);
    }

    fn console_debug(&mut self, log: String) {
        self.logs.push(Log::new(LogLevel::Debug, log));
    }

    fn console_error(&mut self, log: String) {
        self.logs.push(Log::new(LogLevel::Error, log));
    }
}

pub fn start(
    handle_url: fn(String) -> Result<RenderFrame<TuiBrowser>, String>,
) -> Result<(), Box<dyn Error>> {
    // set up terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    execute!(stdout, Clear(ClearType::All))?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // never return unless a user quit the browser app
    let res = run_app(
        Rc::new(RefCell::new(TuiBrowser::new())),
        handle_url,
        &mut terminal,
    );

    // restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{:?}", err)
    }

    Ok(())
}

fn run_app<B: Backend>(
    browser: Rc<RefCell<TuiBrowser>>,
    handle_url: fn(String) -> Result<RenderFrame<TuiBrowser>, String>,
    terminal: &mut Terminal<B>,
) -> Result<(), String> {
    loop {
        match terminal.draw(|frame| ui(browser.clone(), frame)) {
            Ok(_) => {}
            Err(e) => return Err(format!("{:?}", e)),
        }

        let event = match event::read() {
            Ok(event) => event,
            Err(e) => return Err(format!("{:?}", e)),
        };

        let current_input_mode = browser.borrow().input_mode;

        if let Event::Key(key) = event {
            match current_input_mode {
                InputMode::Normal => match key.code {
                    KeyCode::Char('e') => {
                        browser.borrow_mut().input_mode = InputMode::Editing;
                    }
                    KeyCode::Char('q') => {
                        return Ok(());
                    }
                    _ => {}
                },
                InputMode::Editing => match key.code {
                    KeyCode::Enter => {
                        let url: String = browser.borrow_mut().input_url.drain(..).collect();
                        browser.borrow_mut().contents.push(url.clone());
                        match handle_url(url.clone()) {
                            Ok(mut render_frame) => {
                                render_frame.set_ui_object(browser.clone());

                                // for test to output debug messages.
                                print_layout_tree(&browser, &render_frame.layout_object_root(), 0);

                                browser.borrow_mut().render_frame = Some(render_frame);
                            }
                            Err(msg) => {
                                browser.borrow_mut().console_error(msg.clone());
                                return Err(msg);
                            }
                        }
                    }
                    KeyCode::Char(c) => {
                        browser.borrow_mut().input_url.push(c);
                    }
                    KeyCode::Backspace => {
                        browser.borrow_mut().input_url.pop();
                    }
                    KeyCode::Esc => {
                        browser.borrow_mut().input_mode = InputMode::Normal;
                    }
                    _ => {}
                },
            }
        }
    }
}

fn ui<B: Backend>(browser: Rc<RefCell<TuiBrowser>>, frame: &mut Frame<B>) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        //.margin(2)
        .constraints(
            [
                Constraint::Percentage(5),
                Constraint::Percentage(5),
                Constraint::Percentage(50),
                Constraint::Percentage(40),
            ]
            .as_ref(),
        )
        .split(frame.size());

    let (msg, style) = match browser.borrow().input_mode {
        InputMode::Normal => (
            vec![
                Span::raw("Press "),
                Span::styled("q", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to exit, "),
                Span::styled("e", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to start editing."),
            ],
            Style::default().add_modifier(Modifier::RAPID_BLINK),
        ),
        InputMode::Editing => (
            vec![
                Span::raw("Press "),
                Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to stop editing, "),
                Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to record the message"),
            ],
            Style::default(),
        ),
    };
    let mut text = Text::from(Spans::from(msg));
    text.patch_style(style);
    let help_message = Paragraph::new(text);
    frame.render_widget(help_message, chunks[0]);

    // box for url bar
    {
        let b = browser.borrow();
        let input = Paragraph::new(b.input_url.as_ref())
            .style(match browser.borrow().input_mode {
                InputMode::Normal => Style::default(),
                InputMode::Editing => Style::default().fg(Color::Yellow),
            })
            .block(Block::default().borders(Borders::ALL).title("URL"));
        frame.render_widget(input, chunks[1]);
    }
    match browser.borrow().input_mode {
        InputMode::Normal =>
            // Hide the cursor. `Frame` does this by default, so we don't need to do anything here
            {}

        InputMode::Editing => {
            // Make the cursor visible and ask tui-rs to put it at the specified coordinates after rendering
            frame.set_cursor(
                // Put cursor past the end of the input text
                chunks[1].x + browser.borrow().input_url.width() as u16 + 1,
                // Move one line down, from the border to the input line
                chunks[1].y + 1,
            )
        }
    }

    // box for main content
    let contents: Vec<ListItem> = browser
        .borrow()
        .contents
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let content = vec![Spans::from(Span::raw(format!("{}: {}", i, m)))];
            ListItem::new(content)
        })
        .collect();
    let contents =
        List::new(contents).block(Block::default().borders(Borders::ALL).title("Content"));
    frame.render_widget(contents, chunks[2]);

    // box for console logs
    let logs: Vec<ListItem> = browser
        .borrow()
        .logs
        .iter()
        .enumerate()
        .map(|(_, log)| {
            let content = vec![Spans::from(Span::raw(format!("{}", log.to_string())))];
            ListItem::new(content)
        })
        .collect();
    let logs = List::new(logs).block(Block::default().borders(Borders::ALL).title("Console"));
    frame.render_widget(logs, chunks[3]);
}
