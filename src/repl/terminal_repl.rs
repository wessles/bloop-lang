use crate::repl::repl_interpreter::Interpreter;
use crate::repl::history::History;
use crossterm::event::{Event, KeyCode, KeyModifiers, ModifierKeyCode};
use crossterm::execute;
use std::io;
use std::io::{stdout, Write};

pub(crate) fn repl() -> io::Result<()> {
    let mut interpreter = Interpreter::new();
    let mut history = History::new();

    let mut exit_requested = false;
    while !exit_requested {
        let input = {
            crossterm::terminal::enable_raw_mode()?;

            let prefix = "$ ";

            let mut input = String::new();
            let mut cursor = 0;

            loop {
                // render the terminal
                execute!(
                    stdout(),
                    crossterm::cursor::MoveToColumn(0),
                    crossterm::terminal::Clear(crossterm::terminal::ClearType::CurrentLine)
                )?;

                write!(stdout(), "{}{}", prefix, input)?;
                stdout().flush()?;

                execute!(
                    stdout(),
                    crossterm::cursor::MoveToColumn((prefix.len() + cursor) as u16)
                )?;
                stdout().flush()?;

                // handle input
                if let Event::Key(event) = crossterm::event::read()? {
                    match event.code {
                        KeyCode::Esc => {
                            return Ok(());
                        }
                        KeyCode::Modifier(ModifierKeyCode::LeftControl) => {}
                        KeyCode::Char(c) => {
                            if c == 'c' && event.modifiers.contains(KeyModifiers::CONTROL) {
                                exit_requested = true;
                                break;
                            }
                            input.insert(cursor, c);
                            cursor += 1;
                        }
                        KeyCode::Left => {
                            cursor = cursor.saturating_sub(1);
                        }
                        KeyCode::Right => {
                            if cursor < input.len() {
                                cursor += 1;
                            }
                        }
                        KeyCode::Backspace => {
                            if cursor > 0 {
                                input.remove(cursor - 1);
                                cursor -= 1;
                            }
                        }
                        KeyCode::Up => {
                            if let Some(recalled) = history.up() {
                                input = recalled;
                                if cursor > input.len() {
                                    cursor = input.len();
                                }
                            }
                        }
                        KeyCode::Down => {
                            if let Some(recalled) = history.down() {
                                input = recalled;
                                if cursor > input.len() {
                                    cursor = input.len();
                                }
                            }
                        }
                        KeyCode::Enter => {
                            break;
                        }
                        _ => {}
                    }
                }
            }
            crossterm::terminal::disable_raw_mode()?;
            println!();
            io::stdout().flush()?;

            history.push(input.clone());
            input
        };

        let result = match interpreter.interpret(&input) {
            Ok(result) => result,
            Err(err) => {
                eprintln!("{}", err);
                continue;
            }
        };

        print!("{}", result.output);
        if let Some(value) = result.value {
            println!("{}", value);
        }
        stdout().flush()?;
    }
    Ok(())
}
