use core::time;
use std::cmp::min;
use std::io::{stdin, stdout, Write, Stdout};
use std::process::ExitCode;
use std::thread;
use termion::event::{Event, Key};
use termion::input::{Events, TermRead};
use termion::raw::{IntoRawMode, RawTerminal};
use termion::{self, AsyncReader};

struct FuzzyMatcher {
    cursor: usize,
    events: Events<AsyncReader>,
    first_iter: bool,
    height: usize,
    items: Vec<String>,
    matches: Vec<String>,
    offset: usize,
    query: String,
    stdout: RawTerminal<Stdout>,
}

enum HandleEventResult {
    Done,
    NoMatch,
    Quit,
    Continue,
}

impl FuzzyMatcher {
    fn new() -> Self {
        Self {
            cursor: 0,
            events: termion::async_stdin().events(),
            first_iter: true,
            height: 10,
            items: Vec::new(),
            matches: Vec::new(),
            offset: 0,
            query: String::new(),
            stdout:
            match stdout().into_raw_mode() {
                Ok(stdout) => stdout,
                Err(_) => todo!()
            },
        }
    }

    fn read_input(&mut self) {
        for line in stdin().lines() {
            let l = line.expect("not a line?");
            self.items.push(l);
        }
    }

    fn handle_event(&mut self, event: &Option<Result<Event, std::io::Error>>) -> HandleEventResult {
        match event {
            Some(Ok(Event::Key(key))) => {
                match key {
                    Key::Char('\n') if self.matches.len() == 0 => {
                        return HandleEventResult::NoMatch
                    }
                    Key::Char('\n') => {
                        return HandleEventResult::Done;
                    }
                    Key::Ctrl('c') => return HandleEventResult::Quit,
                    // TODO: Why doesnt Key::Up/Down register any more?
                    Key::Ctrl('p') if self.cursor > 0 => self.cursor -= 1,
                    Key::Ctrl('p') if self.offset > 0 => self.offset -= 1,
                    Key::Ctrl('n') if self.cursor < self.height - 1 => self.cursor += 1,
                    Key::Ctrl('n') if self.offset < self.matches.len() - self.height => {
                        self.offset += 1
                    }
                    Key::Backspace if self.query.len() > 0 => {
                        self.query.pop().unwrap();
                    }
                    Key::Char(c) => self.query.push(*c),
                    _ => (),
                }
            }
            _ => (),
        }
        HandleEventResult::Continue
    }

    fn clear_lines(&mut self) {
        for _ in 0..self.height + 1 {
            write!(self.stdout, "{}\n\r", termion::clear::CurrentLine).unwrap();
        }
        self.move_cursor_to_top();
    }

    fn find_matches(&self) -> Vec<String> {
        let mut matches: Vec<String> = Vec::new();
        for item in &self.items {
            if is_match(&item, &self.query) {
                matches.push(item.to_string())
            }
        }
        matches
    }

    fn print_prompt(&mut self) {
        write!(
            self.stdout,
            "$ {} {}[{}/{}]{}\n\r",
            self.query,
            termion::style::Faint,
            self.matches.len(),
            self.items.len(),
            termion::style::Reset
        )
        .unwrap();
    }

    fn print_matches(&mut self) {
        for i in 0..self.height {
            if let Some(m) = self.matches.get(i + self.offset) {
                // TODO: Handle fit on screen better..
                // TODO: Hilight matching text
                let (cols, _) = termion::terminal_size().unwrap();
                let w: usize = min((cols - 10).into(), m.len());
                if self.cursor == i {
                    write!(self.stdout, ">{}", termion::style::Bold).unwrap();
                } else {
                    write!(self.stdout, " ").unwrap();
                }
                write!(
                    self.stdout,
                    " {} {}{}\n\r",
                    i + self.offset,
                    &m.to_string()[..w],
                    termion::style::Reset
                )
                .unwrap();
            } else {
                write!(self.stdout, "{}\n\r", termion::clear::CurrentLine).unwrap();
            }
        }
    }

    fn move_cursor_to_top(&mut self) {
        write!(
            self.stdout,
            "{}",
            termion::cursor::Up((self.height + 1).try_into().unwrap()),
        )
        .unwrap();
    }

    fn adjust_cursor(&mut self) {
        if self.matches.len() > 0 && self.cursor >= self.matches.len() {
            self.cursor = self.matches.len() - 1
        }
    }

    fn adjust_offset(&mut self) {
        if self.offset > self.matches.len() {
            self.offset = 0;
        }
    }

    fn render(&mut self) {
        self.clear_lines();
        self.adjust_cursor();
        self.adjust_offset();
        self.print_prompt();
        self.print_matches();
        self.move_cursor_to_top();
    }

    fn main(&mut self) -> ExitCode {
        loop {
            let ten_millis = time::Duration::from_millis(1);
            thread::sleep(ten_millis);
            // TODO: Figure out how to receive windows resize event
            let event = self.events.next();
            match self.handle_event(&event) {
                HandleEventResult::Done => {
                    // TODO: Make cursor appear in correct place
                    write!(
                        self.stdout,
                        "{}{}\n\r",
                        termion::clear::CurrentLine,
                        self.matches[self.cursor + self.offset]
                    )
                    .unwrap();
                    return ExitCode::SUCCESS;
                }
                HandleEventResult::NoMatch => return ExitCode::FAILURE,
                HandleEventResult::Quit => return ExitCode::from(130),
                HandleEventResult::Continue => (),
            }
            if event.is_some() || self.first_iter {
                self.matches = self.find_matches();
                self.render();
                self.first_iter = false;
            }
        }
    }
}

fn main() -> ExitCode {
    let mut m = FuzzyMatcher::new();
    m.read_input();
    m.main()
}

fn is_match(item: &str, query: &str) -> bool {
    item.contains(query)
}
