use std::cmp::min;
use std::io::{stdin, Stderr, self};
use std::process::ExitCode;
use std::time::{Duration, Instant};
use std::{thread, fs};
use std::io::Write;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::style::{Print, Attribute, SetAttribute, Stylize};
use crossterm::terminal::{ClearType};
use crossterm::{terminal, execute, cursor};
use clap::Parser;
use crossterm::tty::IsTty;
use std::sync::mpsc::{Sender, Receiver};
use std::sync::mpsc;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    fullscreen: bool,
    #[arg(short, long, default_value_t = 10)]
    height: usize
}

struct FuzzyMatcher {
    items: Vec<String>,
    query: Query,
    outstream: Stderr,
    args: Args,
    match_list: MatchList,
    items_receiver: Receiver<String>,
    last_render: Instant,
    need_update: bool
}

enum HandleEventResult {
    Done,
    NoMatch,
    Quit,
    Continue,
}

struct MatchList {
    pub height: usize,
    cursor: usize,
    offset: usize,
    pub matches: Vec<String>
}

impl MatchList {
    fn new(height: usize) -> Self {
        Self{height, cursor: 0, offset: 0, matches: Vec::new()}
    }

    fn get_selection(&self) -> &String {
        &self.matches[self.cursor + self.offset]
    }

    fn move_up_page(&mut self) {
        if self.cursor > 0 {
            self.cursor = self.cursor.saturating_sub(self.height - 1);
        } else if self.offset > 0 {
            self.offset -= self.height - 1
        }
    }

    fn move_down_page(&mut self) {
        if self.cursor < self.height - 1 {
            self.cursor += self.height - 1;
            if self.cursor > self.height - 1 {
                self.cursor = self.height - 1
            }
        } else if self.offset < self.matches.len() - self.height {
            self.offset += self.height - 1
        }
    }

    fn move_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1
        } else if self.offset > 0 {
            self.offset -= 1
        }
    }

    fn move_down(&mut self) {
        if self.cursor < self.height - 1 {
            self.cursor += 1
        } else if self.offset < self.matches.len() - self.height {
            self.offset += 1
        }
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

    fn render(&mut self, mut outstream: &Stderr, query: &Query) {
        self.adjust_cursor();
        self.adjust_offset();
        for i in 0..self.height {
            if let Some(m) = self.matches.get(i + self.offset) {
                let (cols, _rows) = terminal::size().unwrap();
                let w: usize = min((cols - 10).into(), m.len());
                if self.cursor == i {
                    execute!(outstream, Print(">".red()),
                             SetAttribute(Attribute::Bold)).unwrap();
                } else {
                    write!(outstream, " ").unwrap();
                }
                let mut match_str = m.to_string()[..w].to_string();
                for query_part in &query.query {
                    if let Some(begin) = match_str.find(query_part) {
                        let end = begin+query_part.len();
                        match_str = format!("{}{}{}",
                                            &match_str[..begin],
                                            &match_str[begin..end].dark_cyan(),
                                            &match_str[end..]
                        );
                    }
                }
                write!(outstream, " {} {}\n\r",
                       i + self.offset,
                       &match_str,
                ).unwrap();

                execute!(outstream,
                         SetAttribute(Attribute::Reset)).unwrap();
            } else {
                execute!(outstream,
                         terminal::Clear(ClearType::CurrentLine),
                         Print("\n\r")).unwrap();
            }
        }
    }
}

impl FuzzyMatcher {
    fn new(args: Args) -> Self {
        terminal::enable_raw_mode().unwrap();
        let mut stderr = io::stderr();
        let term_height: usize = terminal::size().unwrap().1.checked_sub(1)
            .unwrap().try_into().unwrap();
        let mut height: usize = min(args.height, term_height);
        if args.fullscreen {
            // Set height to the height of the terminal
            height = term_height;
            execute!(stderr, terminal::EnterAlternateScreen).unwrap();
        }
        execute!(stderr, cursor::Hide).unwrap();
        let (items_sender, items_receiver): (Sender<String>, Receiver<String>) =
            mpsc::channel();
        thread::spawn(move || {
            FuzzyMatcher::read_input(items_sender);
        });
        Self {
            args,
            match_list: MatchList::new(height),
            items: Vec::new(),
            query: Query::new(String::new()),
            outstream: stderr,
            items_receiver,
            last_render: Instant::now(),
            need_update: true
        }
    }

    fn read_input(items_sender: Sender<String>) {
        //TODO: Read async
        if stdin().is_tty() {
            //push every filename in current dirlisting to self.items
            for entry in fs::read_dir(".").unwrap() {
                let filename = entry.unwrap().file_name();
                let filename_str = filename.to_string_lossy();
                items_sender.send(filename_str.into_owned()).unwrap();
            }
            return
        }
        for line in stdin().lines() {
            let l = line.expect("not a line?");
            items_sender.send(l).unwrap();
        }
    }

    fn handle_event(&mut self, event: &crossterm::Result<Event>) -> HandleEventResult {
        if let Ok(Event::Key(KeyEvent{code, modifiers, ..})) = event {
            match (code, modifiers) {
                (KeyCode::Enter, _) =>
                    if self.match_list.matches.len() == 0 {
                        return HandleEventResult::NoMatch
                    } else {
                        return HandleEventResult::Done
                    }
                (KeyCode::Char('c'), &KeyModifiers::CONTROL) =>
                    return HandleEventResult::Quit,
                (KeyCode::Char('p'), &KeyModifiers::CONTROL) =>
                    self.match_list.move_up(),
                (KeyCode::Char('n'), &KeyModifiers::CONTROL) =>
                    self.match_list.move_down(),
                (KeyCode::PageUp, _) => self.match_list.move_up_page(),
                (KeyCode::PageDown, _) => self.match_list.move_down_page(),
                (KeyCode::Up, _) => self.match_list.move_up(),
                (KeyCode::Down, _) => self.match_list.move_down(),
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
        for _ in 0..self.match_list.height {
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

            Print(format!(" [{}/{}]", self.match_list.matches.len(), self.items.len()).dim())
        )
        .unwrap();
    }


    fn move_cursor_to_top(&mut self) {
        execute!(self.outstream,
                 cursor::MoveUp((self.match_list.height).try_into().unwrap())
        ).unwrap();
    }

    fn render(&mut self) {
        if self.last_render.elapsed().as_millis() > 10 {
            self.clear_lines();
            self.match_list.render(&self.outstream, &self.query);
            self.print_prompt();
            self.move_cursor_to_top();
            self.last_render = Instant::now();
            self.need_update = false;
        }
    }

    fn main(&mut self) -> ExitCode {
        // TODO: Move main loop
        loop {
            let begin_recv = std::time::Instant::now();
            loop {
                if let Ok(item) = self.items_receiver.recv() {
                    self.items.push(item);
                    self.need_update = true;
                } else {
                    break;
                }
                if begin_recv.elapsed().as_millis() > 10 {
                    break;
                }
            }
            // TODO: Figure out how to receive windows resize event
            if let Ok(true) = crossterm::event::poll(Duration::from_micros(10)) {
                let event = crossterm::event::read();
                match self.handle_event(&event) {
                    HandleEventResult::Done => {
                        self.restore_terminal();
                        println!("{}", self.match_list.get_selection());
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
                    HandleEventResult::Continue =>
                        self.need_update = true
                }
            }
            if self.need_update {
                self.match_list.matches = self.find_matches();
                self.render();
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
        execute!(self.outstream,
                 terminal::Clear(ClearType::CurrentLine),
                 cursor::MoveToColumn(0)
        ).unwrap();
        execute!(self.outstream, cursor::Show).unwrap()
    }
}

fn main() -> ExitCode {
    let args = Args::parse();
    let mut m = FuzzyMatcher::new(args);
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
