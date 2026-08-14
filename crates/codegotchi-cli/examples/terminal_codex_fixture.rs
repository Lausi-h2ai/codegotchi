use std::{error::Error, io, thread, time::Duration};

use codegotchi_cli::terminal::{
    CodexScreen, CrosstermTerminal, TerminalBackend, TerminalGuard, TerminalRunError, render_codex,
};
use ratatui::{Terminal, backend::CrosstermBackend, layout::Position};

const FIXTURE_BYTES: &[u8] = b"\x1b[2J\x1b[H\x1b[48;5;236m\x1b[38;5;15mCodeGotchi Codex VT renderer fixture\x1b[0m\r\n\r\n\x1b[1;31;44mBOLD ANSI 0-15\x1b[0m  \x1b[2;38;5;200mDIM INDEXED 256\x1b[0m\r\n\x1b[3;38;2;94;234;212mITALIC TRUECOLOR\x1b[0m  \x1b[4;38;5;11mUNDERLINE\x1b[0m\r\n\x1b[7;38;5;16;48;5;231mINVERSE + ERASE BG\x1b[0m\r\nUnicode: \x1b[38;5;45m\xE7\x95\x8Ce\xCC\x81\x1b[0m  combining + wide\r\n\x1b[?25l\x1b[2K\x1b[7;1Hcursor hidden while this row is erased\x1b[?25h\x1b[8;1HVISIBLE CURSOR >";

fn main() {
    if let Err(error) = run_fixture() {
        eprintln!("terminal_codex_fixture: {error}");
        std::process::exit(1);
    }
}

fn run_fixture() -> Result<(), Box<dyn Error>> {
    // This fixture is visual proof and must retain colors even under ambient NO_COLOR.
    // Production CodeGotchi rendering continues to respect the user's setting.
    crossterm::style::force_color_output(true);

    let result = TerminalGuard::run_with(CrosstermTerminal::new(), |guard| {
        draw_fixture(guard).map_err(|error| io::Error::other(error.to_string()))
    });

    match result {
        Ok(()) => Ok(()),
        Err(TerminalRunError::Initialization(error)) => Err(Box::new(error)),
        Err(TerminalRunError::Body { error, restoration }) => {
            if let Some(restoration) = restoration {
                Err(Box::new(io::Error::other(format!(
                    "fixture failed: {error}; restoration failed: {restoration}"
                ))))
            } else {
                Err(Box::new(error))
            }
        }
        Err(TerminalRunError::Restoration(error)) => Err(Box::new(error)),
    }
}

fn draw_fixture(guard: &mut TerminalGuard<CrosstermTerminal>) -> Result<(), Box<dyn Error>> {
    let (columns, rows) = guard.backend_mut().size()?;
    let mut screen = CodexScreen::new(rows, columns);
    screen.process(FIXTURE_BYTES);

    let backend = CrosstermBackend::new(guard.writer_mut());
    let mut terminal = Terminal::new(backend)?;
    terminal.draw(|frame| {
        let cursor = render_codex(&screen, frame.area(), frame.buffer_mut());
        if let Some(position) = cursor {
            frame.set_cursor_position(Position::new(position.x, position.y));
        }
    })?;

    let milliseconds = std::env::var("CODEGOTCHI_TERMINAL_FIXTURE_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(3_000);
    thread::sleep(Duration::from_millis(milliseconds));
    drop(terminal);
    Ok(())
}
