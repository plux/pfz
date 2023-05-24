use core::time;
use std::cmp::min;
use std::io::{stdin, Stderr, self};
use std::process::ExitCode;
use std::thread;
use std::io::Write;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::style::{Print, Attribute, SetAttribute, Stylize};
use crossterm::terminal::{ClearType};
use crossterm::{terminal, execute, cursor};

struct FuzzyMatcher {
    cursor: usize,
    height: usize,
    items: Vec<String>,
    matches: Vec<String>,
    offset: usize,
    query: String,
    outstream: Stderr
}

enum HandleEventResult {
    Done,
    NoMatch,
    Quit,
    Continue,
}

impl FuzzyMatcher {
    fn new() -> Self {
        terminal::enable_raw_mode().unwrap();
        let mut stderr = io::stderr();
        execute!(stderr,
                 terminal::EnterAlternateScreen,
                 cursor::Hide,
                 cursor::MoveTo(0,0)).unwrap();
        Self {
            cursor: 0,
            height: 10,
//            height: (terminal::size().unwrap().1 - 2).try_into().unwrap(),
            items: Vec::new(),
            matches: Vec::new(),
            offset: 0,
            query: String::new(),
            outstream: stderr
        }
    }

    fn read_input(&mut self) {
        for line in stdin().lines() {
            let l = line.expect("not a line?");
            self.items.push(l);
        }
    }

    fn handle_event(&mut self, event: &crossterm::Result<Event>) -> HandleEventResult {
        if let Ok(Event::Key(KeyEvent{code, modifiers, ..})) = event {
            match (code, modifiers) {
                (KeyCode::Enter, _) =>
                    if self.matches.len() == 0 {
                        return HandleEventResult::NoMatch
                    } else {
                        return HandleEventResult::Done
                    }
                (code, modifiers)
                    if modifiers == &KeyModifiers::CONTROL
                    || code == &KeyCode::Up || code == &KeyCode::Down
                    => {
                    // TODO: Add support for arrows in a nicer way
                    match code {
                        KeyCode::Char('c') =>
                            return HandleEventResult::Quit,
                        KeyCode::Char('p') | KeyCode::Up if self.cursor > 0 =>
                            self.cursor -= 1,
                        KeyCode::Char('p') | KeyCode::Up if self.offset > 0 =>
                            self.offset -= 1,
                        KeyCode::Char('n') | KeyCode::Down if self.cursor < self.height - 1 =>
                            self.cursor += 1,
                        KeyCode::Char('n') | KeyCode::Down
                            if self.offset < self.matches.len() - self.height =>
                            self.offset += 1,
                        _ => ()
                    }
                }
                (KeyCode::Char(c), _) =>
                    self.query.push(*c),
                (KeyCode::Backspace, _) if self.query.len() > 0 => {
                    self.query.pop().unwrap();
                }
                _ => ()
            }
        }
        HandleEventResult::Continue
    }

    fn clear_lines(&mut self) {
        for _ in 0..self.height + 1 {
            execute!(self.outstream,
                     terminal::Clear(ClearType::CurrentLine),
                     Print("\n\r")
            ).unwrap();
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
        execute!(
            self.outstream,
            Print(format!("$ {} ", self.query)),
            Print(format!("[{}/{}]\n\r", self.matches.len(), self.items.len()).dim())
        )
        .unwrap();
    }

    fn print_matches(&mut self) {
        for i in 0..self.height {
            if let Some(m) = self.matches.get(i + self.offset) {
                // TODO: Hilight matching text
                let (cols, _rows) = terminal::size().unwrap();
                let w: usize = min((cols - 10).into(), m.len());
                if self.cursor == i {
                    execute!(self.outstream, Print(">".red()),
                             SetAttribute(Attribute::Bold)).unwrap();
                } else {
                    write!(self.outstream, " ").unwrap();
                }
                write!(self.outstream,
                       " {} {}\n\r",
                       i + self.offset,
                       &m.to_string()[..w]).unwrap();
                execute!(self.outstream,
                         SetAttribute(Attribute::Reset)).unwrap();
            } else {
                execute!(self.outstream,
                         terminal::Clear(ClearType::CurrentLine),
                         Print("\n\r")).unwrap();
            }
        }
    }

    fn move_cursor_to_top(&mut self) {
        execute!(self.outstream,
                 cursor::MoveUp((self.height + 1).try_into().unwrap())
        ).unwrap();
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

        // TODO: Move main loop
        loop {
            self.matches = self.find_matches();
            self.render();
            let ten_millis = time::Duration::from_millis(1);
            thread::sleep(ten_millis);
            // TODO: Figure out how to receive windows resize event
            let event = crossterm::event::read();
            match self.handle_event(&event) {
                HandleEventResult::Done => {
                    self.restore_terminal();
                    println!("{}",
                             self.matches[self.cursor + self.offset]);
                    return ExitCode::SUCCESS;
                }
                HandleEventResult::NoMatch => {
                    self.restore_terminal();
                    return ExitCode::FAILURE
                }
                HandleEventResult::Quit => {
                    self.restore_terminal();
                    return ExitCode::from(130)
                }
                HandleEventResult::Continue => (),
            }
        }
    }
    fn restore_terminal(&mut self) {
        terminal::disable_raw_mode().unwrap();
        execute!(
            self.outstream,
            terminal::LeaveAlternateScreen,
            cursor::Show,
        ).unwrap();
        //terminal.show_cursor()?;
    }
}

impl Default for FuzzyMatcher {
    fn default() -> Self {
        Self::new()
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
