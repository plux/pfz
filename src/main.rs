use core::time;
use std::cmp::min;
use std::io::{stdin, Stderr, self};
use std::process::ExitCode;
use std::{thread, fs};
use std::io::Write;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::style::{Print, Attribute, SetAttribute, Stylize};
use crossterm::terminal::{ClearType};
use crossterm::{terminal, execute, cursor};
use clap::Parser;
use crossterm::tty::IsTty;
use itertools::Itertools;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    fullscreen: bool
}

struct FuzzyMatcher {
    cursor: usize,
    height: usize,
    items: Vec<String>,
    matches: Vec<String>,
    offset: usize,
    query: Query,
    outstream: Stderr,
    args: Args
}

enum HandleEventResult {
    Done,
    NoMatch,
    Quit,
    Continue,
}

impl FuzzyMatcher {
    fn new(args: Args) -> Self {
        terminal::enable_raw_mode().unwrap();
        let mut stderr = io::stderr();
        let height: usize = if args.fullscreen {
            // Set height to the height of the terminal
            terminal::size().unwrap().1.checked_sub(1).unwrap().try_into().unwrap()
        } else {
            // TODO: Allow reading height from args
            10
        };
        if args.fullscreen {
            execute!(stderr, terminal::EnterAlternateScreen).unwrap();
        }
        execute!(stderr, cursor::Hide).unwrap();
        Self {
            args,
            cursor: 0,
            height,
            items: Vec::new(),
            matches: Vec::new(),
            offset: 0,
            query: Query::new(String::new()),
            outstream: stderr
        }
    }

    fn read_input(&mut self) {
        //TODO: Read async
        if stdin().is_tty() {
            //push every filename in current dirlisting to self.items
            for entry in fs::read_dir(".").unwrap() {
                let filename = entry.unwrap().file_name();
                let filename_str = filename.to_string_lossy();
                self.items.push(filename_str.into_owned());
            }
            return
        }
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
                (KeyCode::Char(c), _) => {
                    // TODO: Move inside Query
                    let mut query_str = self.query.query_str.to_string();
                    query_str.push(*c);
                    self.query = Query::new(query_str.to_string())
                }
                (KeyCode::Backspace, _) if self.query.query_str.len() > 0 => {
                    let mut query_str = self.query.query_str.to_string();
                    query_str.pop().unwrap();
                    self.query = Query::new(query_str);
                }
                _ => ()
            }
        }
        HandleEventResult::Continue
    }

    fn clear_lines(&mut self) {
        for _ in 0..self.height {
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
            if self.query.is_match(&item) {
                matches.push(item.to_string())
            }
        }
        matches
    }

    fn print_prompt(&mut self) {
        execute!(
            self.outstream,
            terminal::Clear(ClearType::CurrentLine),
            Print(format!("> {}", self.query.query_str)),
            Print(" ".negative()),

            Print(format!(" [{}/{}]", self.matches.len(), self.items.len()).dim())
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
                let mut match_str = m.to_string()[..w].to_string();
                for query_part in &self.query.query {
                    if let Some(begin) = match_str.find(query_part) {
                        let end = begin+query_part.len();
                        match_str = format!("{}{}{}",
                                            &match_str[..begin],
                                            &match_str[begin..end].dark_cyan(),
                                            &match_str[end..]
                        );
                    }
                }

                write!(self.outstream,
                       " {} {}\n\r",
                       i + self.offset,
                       &match_str,
                       ).unwrap();

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
                 cursor::MoveUp((self.height).try_into().unwrap())
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
        self.print_matches();
        self.print_prompt();
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
                    execute!(self.outstream,
                             terminal::Clear(ClearType::CurrentLine)).unwrap();
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
        if self.args.fullscreen {
            execute!(
                self.outstream,
                terminal::LeaveAlternateScreen
            ).unwrap();
        }
        // Clearing some stuff when not alternative screen is needed
        execute!(self.outstream, cursor::Show).unwrap()
    }
}

fn main() -> ExitCode {
    let args = Args::parse();
    let mut m = FuzzyMatcher::new(args);
    m.read_input();
    m.main()
}


struct Query {
    pub query_str: String,
    pub query: Vec<String>
}

impl Query {
    fn new(query_str: String) -> Self {
        let mut query = Vec::new();
        for query_part in query_str.split_ascii_whitespace() {
            query.push(query_part.to_string())
        }
        Self{query_str, query}
    }

    fn is_match(&self, item: &str) -> bool {
        for query_part in &self.query {
            if !item.contains(query_part) {
                return false
            }
        }
        true
    }
}
