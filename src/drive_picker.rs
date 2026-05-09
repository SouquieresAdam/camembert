use crate::drives::{self, DriveInfo};
use crate::render::palette;
use crate::render::renderer;
use crossterm::cursor::MoveTo;
use crossterm::event::{self, Event, KeyCode, MouseButton, MouseEventKind};
use crossterm::execute;
use crossterm::style::{Attribute, Color, ResetColor, SetAttribute, SetForegroundColor};
use crossterm::terminal::{Clear, ClearType, size as terminal_size};
use std::io::{self, Stdout, Write};
use std::path::PathBuf;

const HEADER_COLOR: Color = Color::Cyan;
const STATUS_COLOR: Color = Color::DarkGrey;
const BAR_WIDTH: usize = 20;
const FIRST_DRIVE_ROW: u16 = 3;

/// Renders an interactive drive picker. Returns `Some(path)` when the user
/// chooses a drive, `None` when they cancel (Q/Esc) or no drives are mounted.
///
/// Layout:
/// ```text
/// 💽 Choisissez un lecteur
///
///   [1] █ C:\  ████████████████░░░░  78.5%   391 GB / 500 GB
///   [2] ▓ D:\  ████░░░░░░░░░░░░░░░░  20.0%    80 GB / 400 GB
///
/// 1-9 ou clic = entrer · Q = annuler
/// ```
pub fn pick_drive(stdout: &mut Stdout) -> io::Result<Option<PathBuf>> {
    let drives = drives::list_drives();
    if drives.is_empty() {
        return Ok(None);
    }

    loop {
        draw(stdout, &drives)?;

        match event::read()? {
            Event::Key(k) => match k.code {
                KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => return Ok(None),
                KeyCode::Char(c) if c.is_ascii_digit() => {
                    let n = c.to_digit(10).unwrap() as usize;
                    if let Some(drive) = drives.get(n.checked_sub(1).unwrap_or(usize::MAX)) {
                        return Ok(Some(drive.path.clone()));
                    }
                }
                _ => {}
            },
            Event::Mouse(m) if matches!(m.kind, MouseEventKind::Down(MouseButton::Left)) => {
                if let Some(drive) = drive_at_row(&drives, m.row) {
                    return Ok(Some(drive.path.clone()));
                }
            }
            Event::Resize(_, _) => {} // redraw on next loop iteration
            _ => {}
        }
    }
}

/// Returns the drive whose listing row was clicked, if `row` falls within the
/// drive list. Pure helper — testable without a terminal.
pub(crate) fn drive_at_row<'a>(drives: &'a [DriveInfo], row: u16) -> Option<&'a DriveInfo> {
    if row < FIRST_DRIVE_ROW {
        return None;
    }
    let index = (row - FIRST_DRIVE_ROW) as usize;
    drives.get(index)
}

fn draw(stdout: &mut Stdout, drives: &[DriveInfo]) -> io::Result<()> {
    execute!(stdout, Clear(ClearType::All), MoveTo(0, 0))?;

    // Header
    execute!(
        stdout,
        SetAttribute(Attribute::Bold),
        SetForegroundColor(HEADER_COLOR)
    )?;
    write!(stdout, "💽 Choisissez un lecteur")?;
    execute!(stdout, SetAttribute(Attribute::Reset), ResetColor)?;

    // Pre-compute the widest path so the bars line up vertically.
    let path_col_width = drives
        .iter()
        .map(|d| d.path.display().to_string().chars().count())
        .max()
        .unwrap_or(0);

    for (i, drive) in drives.iter().enumerate() {
        let row = FIRST_DRIVE_ROW + i as u16;
        execute!(stdout, MoveTo(2, row))?;
        write_drive_line(stdout, i, drive, path_col_width)?;
    }

    let footer_row = FIRST_DRIVE_ROW + drives.len() as u16 + 1;
    execute!(
        stdout,
        MoveTo(0, footer_row),
        SetForegroundColor(STATUS_COLOR)
    )?;
    let footer = "1-9 ou clic = entrer  ·  Q ou Esc = annuler";
    if let Ok((w, _)) = terminal_size() {
        let truncated = truncate_to_chars(footer, w as usize);
        write!(stdout, "{truncated}")?;
    } else {
        write!(stdout, "{footer}")?;
    }
    execute!(stdout, ResetColor)?;
    stdout.flush()
}

fn write_drive_line(
    stdout: &mut Stdout,
    index: usize,
    drive: &DriveInfo,
    path_col_width: usize,
) -> io::Result<()> {
    let color = palette::color_for_slice(index);
    let ratio = drive.ratio();
    let bar = drives::progress_bar(ratio, BAR_WIDTH);
    let path_str = drive.path.display().to_string();
    let pct = ratio * 100.0;
    let used = renderer::humanize_bytes(drive.used);
    let total = renderer::humanize_bytes(drive.total);

    // [N]
    write!(stdout, "[{}] ", index + 1)?;

    // Path, padded to the common column width so the bar starts in the same column.
    execute!(stdout, SetForegroundColor(color))?;
    write!(stdout, "{:<width$}  ", path_str, width = path_col_width)?;

    // Filled portion of the bar in palette color, empty portion in dim grey.
    let filled_count = bar.chars().filter(|&c| c == '█').count();
    let empty_count = bar.chars().count() - filled_count;
    for _ in 0..filled_count {
        write!(stdout, "█")?;
    }
    execute!(stdout, SetForegroundColor(STATUS_COLOR))?;
    for _ in 0..empty_count {
        write!(stdout, "░")?;
    }
    execute!(stdout, ResetColor)?;

    // Numeric summary.
    write!(stdout, "  {pct:>5.1}%   {used} / {total}")?;
    Ok(())
}

fn truncate_to_chars(s: &str, max_chars: usize) -> String {
    let mut out = String::new();
    let mut count = 0;
    for ch in s.chars() {
        if count >= max_chars {
            break;
        }
        out.push(ch);
        count += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drive(letter: char, total: u64, used: u64) -> DriveInfo {
        DriveInfo {
            path: PathBuf::from(format!("{letter}:\\")),
            total,
            used,
        }
    }

    #[test]
    fn drive_at_row_returns_none_above_the_drive_list() {
        let drives = vec![drive('C', 100, 50)];
        assert!(drive_at_row(&drives, 0).is_none());
        assert!(drive_at_row(&drives, FIRST_DRIVE_ROW - 1).is_none());
    }

    #[test]
    fn drive_at_row_returns_the_corresponding_drive_when_inside_the_list() {
        let drives = vec![drive('C', 100, 50), drive('D', 200, 100), drive('E', 300, 150)];
        assert_eq!(drive_at_row(&drives, FIRST_DRIVE_ROW).unwrap().path, PathBuf::from("C:\\"));
        assert_eq!(drive_at_row(&drives, FIRST_DRIVE_ROW + 1).unwrap().path, PathBuf::from("D:\\"));
        assert_eq!(drive_at_row(&drives, FIRST_DRIVE_ROW + 2).unwrap().path, PathBuf::from("E:\\"));
    }

    #[test]
    fn drive_at_row_returns_none_below_the_drive_list() {
        let drives = vec![drive('C', 100, 50)];
        assert!(drive_at_row(&drives, FIRST_DRIVE_ROW + 1).is_none());
        assert!(drive_at_row(&drives, 99).is_none());
    }
}
