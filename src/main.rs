use core::time;
use std::cmp::{max, min};
use std::io::{stdin, stdout, Error, Write};
use std::process::ExitCode;
use std::thread::sleep;
use termion::event::{Event, Key};
use termion::input::TermRead;
use termion::raw::IntoRawMode;
use termion::screen::IntoAlternateScreen;
use termion::{self, terminal_size};
fn main() -> ExitCode {
    let mut items = Vec::new();
    for line in stdin().lines() {
        let l = line.expect("not a line?");
        items.push(l);
    }
    let mut query = String::new();
    let height: usize = 5;
    let mut cursor = 0;
    let mut offset = 0;
    let mut matches: Vec<String> = Vec::new();
    let mut events = termion::async_stdin().events();
    let mut stdout = match stdout().into_raw_mode() {
        Ok(stdout) => stdout,
        Err(_) => todo!(),
    };
    let mut first_iter = true;
    loop {
        // TODO: Figure out how to receive windows resize event
        let event = events.next();
        match event {
            Some(Ok(termion::event::Event::Key(key))) => {
                match key {
                    Key::Char('\n') if matches.len() == 0 => {
                        return ExitCode::from(1)
                    },
                    Key::Char('\n') => {
                        write!(
                            stdout,
                            "{}{}\n\r",
                            termion::clear::CurrentLine,
                            matches[cursor + offset]
                        );
                        // TODO: Make cursor appear in correct place
                        return ExitCode::SUCCESS;
                    }
                    Key::Ctrl('c') => {
                        return ExitCode::from(130)
                    },
                    // TODO: Why doesnt Key::Up/Down register any more?
                    Key::Ctrl('p') if cursor > 0 => {
                        cursor -= 1
                    },
                    Key::Ctrl('p') if offset > 0 => {
                        offset -= 1
                    },
                    Key::Ctrl('n') if cursor < height - 1 => {
                        cursor += 1
                    },
                    Key::Ctrl('n') if offset < matches.len() - height => {
                        offset += 1
                    },
                    Key::Backspace => {
                        _ = query.pop().unwrap_or('\0');
                        ()
                    }
                    Key::Char(c) => {
                        query.push(c)
                    },
                    _ => (),
                }
            }
            _ => (),
        }
        if event.is_some() || first_iter {
            // clear lines
            for _ in 0..height {
                write!(stdout, "{}\n\r", termion::clear::CurrentLine);
            }
            // move back to top
            for _ in 0..height {
                write!(stdout, "{}", termion::cursor::Up(1)).unwrap();
            }

            // find matches
            matches.clear();
            for item in &items {
                if is_match(&item, &query) {
                    matches.push(item.to_string())
                }
            }

            // ensure cursor always points to a match
            if matches.len() > 0 && cursor >= matches.len() {
                cursor = matches.len() - 1
            }

            if offset > matches.len() {
                offset = 0;
            }

            // print query
            write!(
                stdout,
                "\roffset:{offset}/matches:{}/cursor:{cursor} {query}\n\r",
                { matches.len() }
            )
            .unwrap();

            // print matches
            for i in 0..height {
                if let Some(m) = matches.get(i + offset) {
                    // TODO: Handle fit on screen better..
                    // TODO: Hilight matching text
                    let (cols, _) = termion::terminal_size().unwrap();
                    let w: usize = min((cols - 10).into(), m.len());
                    if cursor == i {
                        write!(stdout, ">");
                    } else {
                        write!(stdout, " ");
                    }
                    write!(stdout, " {} {}\n\r", i + offset, &m.to_string()[0..w]);
                } else {
                    write!(stdout, "{}\n\r", termion::clear::CurrentLine);
                }
            }

            write!(
                stdout,
                "{}",
                termion::cursor::Up((height + 1).try_into().unwrap())
            )
            .unwrap();
        }
        first_iter = false;
    }
}

//TODO: Refactor this mess :)

fn is_match(item: &str, query: &str) -> bool {
    item.contains(query)
}
