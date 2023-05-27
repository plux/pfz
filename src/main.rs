use clap::Parser;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::style::{Attribute, Print, SetAttribute, Stylize};
use crossterm::terminal::ClearType;
use crossterm::tty::IsTty;
use crossterm::{cursor, execute, terminal};
use std::cmp::min;
use std::io::Write;
use std::io::{self, stdin, Stderr};
use std::process::ExitCode;
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};
use std::{fs, thread};
use itertools::Itertools;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    fullscreen: bool,
    #[arg(short, long)]
    benchmark: bool,
    #[arg(long, default_value_t = 10)]
    height: usize,
}

struct FuzzyMatcher {
    items: Vec<String>,
    query: Query,
    outstream: Stderr,
    args: Args,
    match_list: MatchList,
    items_receiver: Receiver<Option<Vec<String>>>,
    last_render: Instant,
    screen_size: (usize, usize)
}

enum HandleEventResult {
    Done,
    NoMatch,
    Quit,
    Continue,
    UpdateMatches
}

struct MatchList {
    pub height: usize,
    cursor: usize,
    offset: usize,
    pub matches: Vec<String>,
}

impl MatchList {
    fn new(height: usize) -> Self {
        Self {
            height,
            cursor: 0,
            offset: 0,
            matches: Vec::new(),
        }
    }

    fn get_selection(&self) -> &String {
        &self.matches[self.cursor + self.offset]
    }

    fn move_up_page(&mut self) {
        if self.cursor > 0 {
            self.cursor = self.cursor.saturating_sub(self.height);
        } else if self.offset > 0 {
            self.offset -= self.height
        }
    }

    fn move_down_page(&mut self) {
        if self.cursor < self.height {
            self.cursor += self.height;
            if self.cursor > self.height {
                self.cursor = self.height
            }
        } else if self.offset < self.matches.len() - self.height - 1 {
            self.offset += self.height;
            if self.offset > self.matches.len() - self.height - 1 {
                self.offset = self.matches.len() - self.height - 1;
            }
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
        if self.cursor < self.height {
            self.cursor += 1
        } else if self.offset < self.matches.len() - self.height - 1 {
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
        for i in 0..=self.height {
            if let Some(m) = self.matches.get(i + self.offset) {
                let (cols, _rows) = terminal::size().unwrap();
                let w: usize = min((cols - 10).into(), m.len());
                if self.cursor == i {
                    execute!(outstream, Print(">".red()), SetAttribute(Attribute::Bold)).unwrap();
                } else {
                    write!(outstream, " ").unwrap();
                }
                let mut match_str = m[..w].to_string();
                for query_part in &query.query {
                    if let Some(begin) = match_str.find(query_part) {
                        let end = begin + query_part.len();
                        match_str = format!(
                            "{}{}{}",
                            &match_str[..begin],
                            &match_str[begin..end].dark_cyan(),
                            &match_str[end..]
                        );
                    }
                }
                write!(outstream, " {} {}\n\r", i + self.offset, &match_str,).unwrap();

                execute!(outstream, SetAttribute(Attribute::Reset)).unwrap();
            } else {
                execute!(
                    outstream,
                    terminal::Clear(ClearType::CurrentLine),
                    Print("\n\r")
                )
                .unwrap();
            }
        }
    }
}

impl FuzzyMatcher {
    fn new(args: Args) -> Self {
        terminal::enable_raw_mode().unwrap();
        let mut stderr = io::stderr();
        let term_width: usize = terminal::size().unwrap().0.into();
        let term_height: usize = terminal::size().unwrap().1.into();
        let mut height: usize = min(args.height, term_height - 2);
        if args.fullscreen {
            // Set height to the height of the terminal
            height = term_height - 2;
            execute!(stderr, terminal::EnterAlternateScreen).unwrap();
        }
        execute!(stderr, cursor::Hide).unwrap();
        let (items_sender, items_receiver) = mpsc::channel();
        thread::spawn(|| FuzzyMatcher::read_input(items_sender));
        Self {
            args,
            match_list: MatchList::new(height),
            items: Vec::new(),
            query: Query::new(String::new()),
            outstream: stderr,
            items_receiver,
            last_render: Instant::now(),
            screen_size: (term_width, term_height)
        }
    }

    fn read_input(items_sender: Sender<Option<Vec<String>>>) {
        if stdin().is_tty() {
            // Use current dirlisting as default if there's no piped input
            for entry in fs::read_dir(".").unwrap() {
                let filename = entry.unwrap().file_name();
                let filename_str = filename.to_string_lossy();
                items_sender.send(Some(vec![filename_str.into_owned()])).unwrap();
            }
        } else {
            for chunk in &stdin().lines().map(|line| line.unwrap()).into_iter().chunks(128) {
                items_sender.send(Some(chunk.into_iter().collect_vec())).unwrap()
            }
        }
        items_sender.send(None).unwrap();
    }

    fn handle_event(&mut self, event: &crossterm::Result<Event>) -> HandleEventResult {
        match event {
            Ok(Event::Key(KeyEvent {
                    code, modifiers, ..
                })) => {
                match (code, modifiers) {
                    (KeyCode::Enter, _) => {
                        if self.match_list.matches.len() == 0 {
                            return HandleEventResult::NoMatch;
                        } else {
                            return HandleEventResult::Done;
                        }
                    }
                    (KeyCode::Char('c'), &KeyModifiers::CONTROL) => return HandleEventResult::Quit,
                    (KeyCode::Char('p'), &KeyModifiers::CONTROL) => self.match_list.move_up(),
                    (KeyCode::Char('n'), &KeyModifiers::CONTROL) => self.match_list.move_down(),
                    (KeyCode::PageUp, _) => self.match_list.move_up_page(),
                    (KeyCode::PageDown, _) => self.match_list.move_down_page(),
                    (KeyCode::Up, _) => self.match_list.move_up(),
                    (KeyCode::Down, _) => self.match_list.move_down(),
                    (KeyCode::Char(c), _) => {
                        // TODO: Move inside Query
                        let mut query_str = self.query.query_str.to_string();
                        query_str.push(*c);
                        self.query = Query::new(query_str.to_string());
                        return HandleEventResult::UpdateMatches;
                    }
                    (KeyCode::Backspace, _) if self.query.query_str.len() > 0 => {
                        let mut query_str = self.query.query_str.to_string();
                        query_str.pop().unwrap();
                        self.query = Query::new(query_str);
                        return HandleEventResult::UpdateMatches;
                    }
                    _ => (),
                }
            }
            Ok(Event::Resize(cols, rows)) => {
                let size: (usize, usize) = ((*cols).into(), (*rows).into());
                self.screen_size = size;
            }
            _ => ()
        }
        HandleEventResult::Continue
    }

    fn clear_lines(&mut self) {
        for _ in 0..=self.match_list.height {
            execute!(
                self.outstream,
                terminal::Clear(ClearType::CurrentLine),
                Print("\n\r")
            )
            .unwrap();
        }
        self.move_cursor_to_top();
    }

    fn find_matches(&self) -> Vec<String> {
        if self.query.query.len() == 0 {
            // TODO: Avoid cloning, will make it faster
            self.items.clone()
        } else {
            self.items
                .iter()
                .filter(|&item| self.query.is_match(item))
                .map(|item| item.to_string())
                .collect()
        }
    }
    fn render_prompt(&mut self) {
        let info = format!(" [{}/{}]", self.match_list.matches.len(), self.items.len());
        let prompt = &format!("> {} ",
                             self.query.query_str,
        );
        let w = prompt.len() - min(self.screen_size.0, prompt.len());
        execute!(self.outstream,
                 terminal::Clear(ClearType::CurrentLine),
                 Print(prompt[w..].to_string()),
                 Print(info)
        ).unwrap();
//         execute!(
//             self.outstream,
// /            Print(format!(),
// //            Print(" ".negative()),
//             Print()
//         ).unwrap();
    }

    fn move_cursor_to_top(&mut self) {
        execute!(
            self.outstream,
            cursor::MoveUp((self.match_list.height + 1).try_into().unwrap())
        )
        .unwrap();
    }

    fn render(&mut self) {
        self.clear_lines();
        self.match_list.render(&self.outstream, &self.query);
        self.render_prompt();
        self.move_cursor_to_top();
        self.last_render = Instant::now()
    }

    fn main(&mut self) -> ExitCode {
        // TODO: Move main loop
        let mut update_matches = true;
        let mut update_render = true;
        let mut update_items = true;
        loop {
            if update_items {
                let begin_recv = std::time::Instant::now();
                while begin_recv.elapsed().as_millis() < 30 {
                    match self.items_receiver.recv() {
                        Ok(Some(mut chunk)) => {
                            self.items.append(&mut chunk);
                            update_render = true;
                            update_matches = true;
                        }
                        Ok(None) if self.args.benchmark => {
                            self.restore_terminal();
                            return ExitCode::SUCCESS
                        }
                        _ => update_items = false
                    }
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
                        return ExitCode::FAILURE;
                    }
                    HandleEventResult::Quit => {
                        self.restore_terminal();
                        return ExitCode::from(130);
                    }
                    HandleEventResult::UpdateMatches => update_matches = true,
                    HandleEventResult::Continue => update_render = true,
                }
            }
            if self.last_render.elapsed().as_millis() > 30 {
                if update_matches {
                        self.match_list.matches = self.find_matches()
                }
                if update_render || update_matches {
                    self.render();
                }
                update_render = false;
                update_matches = false;
            }
        }
    }
    fn restore_terminal(&mut self) {
        terminal::disable_raw_mode().unwrap();
        if self.args.fullscreen {
            execute!(self.outstream, terminal::LeaveAlternateScreen).unwrap();
        }
        execute!(
            self.outstream,
            terminal::Clear(ClearType::CurrentLine),
            cursor::MoveToColumn(0)
        )
        .unwrap();
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
        let query: Vec<String> = query_str
            .split_ascii_whitespace()
            .map(|query_part| query_part.to_string())
            .collect();
        Self { query_str, query }
    }

    fn is_match(&self, item: &str) -> bool {
        for query_part in &self.query {
            if !item.contains(query_part) {
                return false;
            }
        }
        true
    }
}
