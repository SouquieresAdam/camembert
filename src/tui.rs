use crate::DiskEntry;
use crate::aggregator;
use crate::command::Command;
use crate::drive_picker;
use crate::event_map::event_to_command;
use crate::render::cheese_spinner;
use crate::render::layout::{self, Layout};
use crate::render::palette;
use crate::render::renderer::{self, ProgressItem, RenderedView};
use crate::scanner::{self, FolderCache, ScanProgress};
use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, MouseEvent,
};
use crossterm::execute;
use crossterm::style::{Color, ResetColor, SetAttribute, SetForegroundColor, Attribute};
use crossterm::terminal::{
    Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
    enable_raw_mode, size as terminal_size,
};
use std::collections::HashMap;
use std::io::{self, Stdout, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, TryRecvError};
use std::thread;
use std::time::Duration;

const HEADER_ROWS: u16 = 1;
const MAX_SLICES: usize = 8;
/// Default legend width used when no previous scan informs the layout.
/// Matches the typical width of `[N] █ Documents 99.9%   123.4 MB`.
const DEFAULT_LEGEND_WIDTH: usize = 32;
const SPLASH_FRAMES: usize = 12;

pub fn run(initial_folder: PathBuf) -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture, Hide)?;

    let _guard = TerminalGuard;
    show_startup_splash(&mut stdout)?;
    main_loop(&mut stdout, initial_folder)
}

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let mut stdout = io::stdout();
        let _ = execute!(stdout, ResetColor, DisableMouseCapture, LeaveAlternateScreen, Show);
        let _ = disable_raw_mode();
    }
}

fn main_loop(stdout: &mut Stdout, mut current_folder: PathBuf) -> io::Result<()> {
    let mut status_message = String::from(
        "1-9 = drill · U = remonter · D = changer de disque · R = refresh · Q = quitter",
    );
    let mut cache = FolderCache::new();
    // The most recently displayed pie. Used during the next scan to keep the
    // legend visible (a "preview"), so the TUI's geometry stays stable.
    let mut last_pie: Vec<DiskEntry> = Vec::new();

    loop {
        let term_size = terminal_size().unwrap_or((80, 24));
        // Choose the geometry BEFORE scanning so the spinner can match the
        // size the eventual pie will have. We bias the slice count and legend
        // width with whatever we knew about the previous folder; on the very
        // first scan we fall back to the worst-case `MAX_SLICES + 1`.
        let scan_pie_len = if last_pie.is_empty() { MAX_SLICES + 1 } else { last_pie.len() };
        let scan_legend_width = if last_pie.is_empty() {
            DEFAULT_LEGEND_WIDTH
        } else {
            renderer::legend_width(&last_pie)
        };
        let scan_layout = layout::compute_layout(
            term_size.0,
            term_size.1,
            scan_pie_len,
            scan_legend_width,
        );

        let pie = match obtain_pie(
            stdout,
            &mut current_folder,
            &mut cache,
            &mut status_message,
            &scan_layout,
            &last_pie,
        )? {
            Some(pie) => pie,
            None => return Ok(()), // user cancelled or unrecoverable error at root
        };

        let mut term_size = term_size;
        let legend_w = renderer::legend_width(&pie);
        let mut layout =
            layout::compute_layout(term_size.0, term_size.1, pie.len(), legend_w);
        let mut view = renderer::render_with_layout(&pie, &layout);
        let mut needs_redraw = true;

        // Inner loop: wait for an action that changes the displayed folder.
        // Mouse moves and other no-op events are silently dropped — no redraw, no rescan.
        loop {
            if needs_redraw {
                draw_view(stdout, &current_folder, &view, &status_message)?;
                needs_redraw = false;
            }

            let raw_event = event::read()?;

            // Terminal resize: recompute layout, re-render, and redraw without changing folder.
            if let Event::Resize(w, h) = raw_event {
                term_size = (w, h);
                layout = layout::compute_layout(term_size.0, term_size.1, pie.len(), legend_w);
                view = renderer::render_with_layout(&pie, &layout);
                needs_redraw = true;
                continue;
            }

            let local_event = shift_event_into_view(raw_event, HEADER_ROWS);

            match event_to_command(&local_event, &view.targets) {
                Some(Command::Quit) => return Ok(()),
                Some(Command::Refresh) => {
                    cache.remove(&current_folder);
                    status_message = "Refresh forcé (cache vidé)".to_string();
                    break;
                }
                Some(Command::Up) => match current_folder.parent() {
                    Some(parent) => {
                        current_folder = parent.to_path_buf();
                        status_message = "↑ remonté".to_string();
                        break;
                    }
                    None => {
                        // Already at the root of a disk — switch to the drive picker
                        // instead of refusing the navigation.
                        match drive_picker::pick_drive(stdout)? {
                            Some(drive) => {
                                current_folder = drive;
                                status_message = format!("→ {}", current_folder.display());
                                break;
                            }
                            None => {
                                status_message =
                                    "(racine du disque · D pour changer de lecteur)".to_string();
                                needs_redraw = true;
                            }
                        }
                    }
                },
                Some(Command::ChangeDrive) => {
                    match drive_picker::pick_drive(stdout)? {
                        Some(drive) => {
                            current_folder = drive;
                            status_message = format!("→ {}", current_folder.display());
                            break;
                        }
                        None => {
                            needs_redraw = true;
                        }
                    }
                }
                Some(Command::DrillInto { slice_number }) => {
                    if let Some(target) = pie.get(slice_number - 1) {
                        if target.is_drillable() {
                            current_folder = current_folder.join(&target.name);
                            status_message = format!("→ {}", target.name);
                            break;
                        } else {
                            status_message = format!(
                                "« {} » n'est pas un dossier — drill-down impossible",
                                target.name
                            );
                            needs_redraw = true;
                        }
                    }
                }
                Some(Command::Unknown(_)) | None => {
                    // Mouse move, scroll, unrecognised key — keep current view, wait again.
                }
            }
        }

        // Keep the just-displayed pie around so the next scan can use it as a
        // preview legend (stable TUI footprint while the next walk runs).
        last_pie = pie;
    }
}

/// Returns the aggregated pie for `current_folder`, using the cache when available
/// and otherwise running a threaded scan with the spinner. Returns `None` to signal
/// "exit the program" (user cancelled the scan, or an error at the disk root).
///
/// `scan_layout` and `preview_pie` are used during the scan to draw a spinner of
/// matching size with the previous folder's legend as a placeholder, so the TUI
/// keeps a stable footprint.
fn obtain_pie(
    stdout: &mut Stdout,
    current_folder: &mut PathBuf,
    cache: &mut FolderCache,
    status_message: &mut String,
    scan_layout: &Layout,
    preview_pie: &[DiskEntry],
) -> io::Result<Option<Vec<DiskEntry>>> {
    if let Some(cached) = cache.get(current_folder) {
        let pie = aggregator::aggregate(cached.clone(), MAX_SLICES);
        *status_message = format!("(cache · {} entrées)", cached.len());
        return Ok(Some(pie));
    }

    let cache_snapshot = cache.clone();
    match scan_with_spinner(stdout, current_folder, scan_layout, preview_pie, cache_snapshot)? {
        ScanOutcome::Done { entries, cache: augmented } => {
            *cache = augmented;
            Ok(Some(aggregator::aggregate(entries, MAX_SLICES)))
        }
        ScanOutcome::Cancelled => Ok(None),
        ScanOutcome::Error(err) => {
            *status_message = format!("Erreur scan : {err} — on remonte");
            if let Some(parent) = current_folder.parent() {
                *current_folder = parent.to_path_buf();
                // Recurse to try the parent. With our small cache it'll usually be a hit.
                obtain_pie(stdout, current_folder, cache, status_message, scan_layout, preview_pie)
            } else {
                Ok(None)
            }
        }
    }
}

// --- Scan with spinner ---

enum ScanOutcome {
    /// Scan completed: the first-level entries for the requested folder, paired
    /// with the full cache (the snapshot we sent in, augmented by everything the
    /// recursion visited). The TUI then replaces its cache with this one.
    Done {
        entries: Vec<DiskEntry>,
        cache: FolderCache,
    },
    Cancelled,
    Error(io::Error),
}

/// Messages the scan thread sends back to the spinner loop.
enum ScanMessage {
    /// Quick `read_dir` of the new folder's immediate children — sizes still 0.
    /// Lets the TUI swap its placeholder legend over to the new level instantly.
    Skeleton(Vec<DiskEntry>),
    /// The recursive walk just descended into this path. Used to update the
    /// "currently scanning…" line in the footer.
    Visiting(PathBuf),
    /// An immediate child of the root finished. Its total size is now known —
    /// the spinner legend can replace its cursor placeholder with the real value.
    Completed { name: String, bytes: u64 },
    /// Full recursive scan completed. Carries the augmented cache (the snapshot
    /// that was passed in plus every new entry the walk populated).
    Done(Vec<DiskEntry>, FolderCache),
    /// Recursive scan failed at the top level (e.g. permission denied).
    Error(io::Error),
}

fn scan_with_spinner(
    stdout: &mut Stdout,
    folder: &Path,
    layout: &Layout,
    fallback_preview: &[DiskEntry],
    cache_snapshot: FolderCache,
) -> io::Result<ScanOutcome> {
    let (sender, receiver) = mpsc::channel();
    let folder_for_thread = folder.to_path_buf();
    thread::spawn(move || {
        // Step 1 — instant skeleton (just names, no sizes).
        if let Ok(skeleton) = scanner::list_first_level(&folder_for_thread) {
            let _ = sender.send(ScanMessage::Skeleton(skeleton));
        }
        // Step 2 — recursive scan with sizes, streaming progress events back.
        // Reuse the main thread's cache snapshot so subtrees we already walked
        // (e.g. the folder we just navigated up from) are instant cache hits
        // rather than redundant disk traffic.
        let progress_sender = sender.clone();
        let mut cache = cache_snapshot;
        let result = scanner::scan_first_level_cached_with_progress(
            &folder_for_thread,
            &mut cache,
            move |progress| match progress {
                ScanProgress::Entered(path) => {
                    let _ = progress_sender.send(ScanMessage::Visiting(path));
                }
                ScanProgress::TopLevelDone { name, bytes } => {
                    let _ = progress_sender.send(ScanMessage::Completed { name, bytes });
                }
            },
        );
        match result {
            Ok(entries) => {
                let _ = sender.send(ScanMessage::Done(entries, cache));
            }
            Err(err) => {
                let _ = sender.send(ScanMessage::Error(err));
            }
        }
    });

    let mut frame_index: usize = 0;
    let mut current_layout = layout.clone();
    let mut skeleton: Option<Vec<DiskEntry>> = None;
    let mut visiting: Option<PathBuf> = None;
    let mut completed: HashMap<String, u64> = HashMap::new();

    loop {
        // Drain any messages that have arrived since the last frame.
        loop {
            match receiver.try_recv() {
                Ok(ScanMessage::Skeleton(s)) => {
                    skeleton = Some(prepare_skeleton(s));
                }
                Ok(ScanMessage::Visiting(path)) => {
                    visiting = Some(path);
                }
                Ok(ScanMessage::Completed { name, bytes }) => {
                    completed.insert(name, bytes);
                }
                Ok(ScanMessage::Done(entries, augmented_cache)) => {
                    return Ok(ScanOutcome::Done {
                        entries,
                        cache: augmented_cache,
                    });
                }
                Ok(ScanMessage::Error(err)) => return Ok(ScanOutcome::Error(err)),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    return Ok(ScanOutcome::Error(io::Error::other(
                        "scan thread terminated unexpectedly",
                    )));
                }
            }
        }

        match skeleton.as_deref() {
            Some(s) => {
                let items = merge_skeleton_with_completed(s, &completed);
                draw_progress_frame(
                    stdout,
                    folder,
                    &current_layout,
                    &items,
                    visiting.as_deref(),
                    frame_index,
                )?;
            }
            None => draw_spinner_frame(
                stdout,
                folder,
                &current_layout,
                fallback_preview,
                visiting.as_deref(),
                frame_index,
            )?,
        }
        frame_index = frame_index.wrapping_add(1);

        if event::poll(Duration::from_millis(cheese_spinner::FRAME_DURATION_MS))? {
            let evt = event::read()?;
            if is_quit_event(&evt) {
                return Ok(ScanOutcome::Cancelled);
            }
            // Resize during scan: recompute layout so the spinner matches the
            // new terminal size from the next frame on. Use whichever preview
            // we currently have to size the legend column.
            if let Event::Resize(w, h) = evt {
                let preview = skeleton.as_deref().unwrap_or(fallback_preview);
                let preview_n = if preview.is_empty() { MAX_SLICES + 1 } else { preview.len() };
                let preview_w = if preview.is_empty() {
                    DEFAULT_LEGEND_WIDTH
                } else {
                    renderer::legend_width(preview)
                };
                current_layout = layout::compute_layout(w, h, preview_n, preview_w);
            }
        }
    }
}

/// Layers the running tally of completed top-level subtrees on top of the
/// alphabetical skeleton so the legend can show real sizes for finished
/// children and a cursor placeholder for the rest.
fn merge_skeleton_with_completed(
    skeleton: &[DiskEntry],
    completed: &HashMap<String, u64>,
) -> Vec<ProgressItem> {
    skeleton
        .iter()
        .map(|e| match completed.get(&e.name) {
            Some(&bytes) => ProgressItem::known(e.name.clone(), e.kind.clone(), bytes),
            None => ProgressItem::pending(e.name.clone(), e.kind.clone()),
        })
        .collect()
}

/// Sorts the skeleton alphabetically and caps it at `MAX_SLICES`, appending an
/// "(autres)" bucket when truncation actually drops anything. This keeps the
/// legend the same shape it'll have once sizes arrive.
fn prepare_skeleton(mut skeleton: Vec<DiskEntry>) -> Vec<DiskEntry> {
    skeleton.sort_by(|a, b| a.name.cmp(&b.name));
    if skeleton.len() > MAX_SLICES {
        let dropped = skeleton.len() - MAX_SLICES;
        skeleton.truncate(MAX_SLICES);
        skeleton.push(DiskEntry::bucket(format!("(et {dropped} autres…)"), 0));
    }
    skeleton
}

fn is_quit_event(event: &Event) -> bool {
    if let Event::Key(key) = event {
        matches!(
            key.code,
            KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc
        )
    } else {
        false
    }
}

// --- Drawing ---

const HEADER_COLOR: Color = Color::Cyan;
const SPINNER_COLOR: Color = Color::Yellow;
const STATUS_COLOR: Color = Color::DarkGrey;

/// User-customizable splash banner, embedded at compile time.
/// Edit `src/splash.txt` and run `cargo install --path . --force` to update.
const SPLASH_BANNER: &str = include_str!("splash.txt");
const SPLASH_DURATION_MS: u64 = SPLASH_FRAMES as u64 * cheese_spinner::FRAME_DURATION_MS;

fn show_startup_splash(stdout: &mut Stdout) -> io::Result<()> {
    if SPLASH_BANNER.trim().is_empty() {
        return Ok(());
    }

    execute!(stdout, Clear(ClearType::All), MoveTo(0, 0), SetForegroundColor(SPINNER_COLOR))?;
    for (row, line) in SPLASH_BANNER.lines().enumerate() {
        execute!(stdout, MoveTo(0, row as u16))?;
        write!(stdout, "{line}")?;
    }
    execute!(stdout, ResetColor)?;
    stdout.flush()?;

    // Hold the banner on screen, but let any keypress skip it.
    if event::poll(Duration::from_millis(SPLASH_DURATION_MS))? {
        let _ = event::read();
    }
    Ok(())
}

fn draw_spinner_frame(
    stdout: &mut Stdout,
    folder: &Path,
    layout: &Layout,
    preview_pie: &[DiskEntry],
    visiting: Option<&Path>,
    frame_index: usize,
) -> io::Result<()> {
    let scan_view = renderer::render_scan_view(layout, preview_pie, frame_index);
    paint_scan_screen(stdout, folder, layout, &scan_view, visiting)
}

fn draw_progress_frame(
    stdout: &mut Stdout,
    folder: &Path,
    layout: &Layout,
    items: &[ProgressItem],
    visiting: Option<&Path>,
    frame_index: usize,
) -> io::Result<()> {
    let scan_view = renderer::render_progress_view(layout, items, frame_index);
    paint_scan_screen(stdout, folder, layout, &scan_view, visiting)
}

fn paint_scan_screen(
    stdout: &mut Stdout,
    folder: &Path,
    layout: &Layout,
    scan_view: &renderer::RenderedView,
    visiting: Option<&Path>,
) -> io::Result<()> {
    execute!(stdout, Clear(ClearType::All))?;
    draw_header(stdout, folder)?;

    let body_lines: Vec<&str> = scan_view.display.lines().collect();
    let (_, pie_height) = layout::pie_dimensions(layout.radius);

    for (i, line) in body_lines.iter().enumerate() {
        execute!(stdout, MoveTo(0, HEADER_ROWS + i as u16))?;
        let cells_row: &[Option<usize>] = scan_view
            .targets
            .cells
            .get(i)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        // Spinner cells (left of the legend in side-by-side, all of the row in
        // stacked) are the visible spinner pixels — paint them yellow. Cells
        // that map to a slice index are preview-legend characters and use the
        // palette as usual.
        write_scan_line(stdout, line, cells_row, i, pie_height, layout.side_by_side)?;
    }

    let footer_row = HEADER_ROWS + body_lines.len() as u16 + 1;
    execute!(stdout, MoveTo(0, footer_row), SetForegroundColor(STATUS_COLOR))?;
    let footer_text = match visiting {
        Some(path) => format!("⟳ {}  (Q = annuler)", path.display()),
        None => String::from("Scan en cours…  (Q pour annuler)"),
    };
    // Truncate to terminal width to avoid wrap, which would push subsequent
    // draws off-screen on narrow terminals.
    if let Ok((w, _)) = crossterm::terminal::size() {
        let truncated = truncate_to_chars(&footer_text, w as usize);
        write!(stdout, "{truncated}")?;
    } else {
        write!(stdout, "{footer_text}")?;
    }
    execute!(stdout, ResetColor)?;
    stdout.flush()
}

/// Returns at most `max_chars` characters from the start of `s`. Long deep paths
/// in the footer would otherwise wrap to a second line and the spinner above
/// would scroll up by one row — visually unstable.
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

/// Paints one line of the scan view: spinner pixels in yellow, preview legend
/// in palette colors. The layout decides which columns are spinner vs legend.
fn write_scan_line(
    stdout: &mut Stdout,
    line: &str,
    cells_row: &[Option<usize>],
    row_idx: usize,
    pie_height: usize,
    side_by_side: bool,
) -> io::Result<()> {
    // In stacked mode, rows < pie_height are the spinner; the rest is legend.
    // In side-by-side, the spinner occupies the left columns of the spinner rows.
    let spinner_row = row_idx < pie_height;
    let mut current: Option<Color> = None;
    let mut started = false;

    for (col, ch) in line.chars().enumerate() {
        let is_spinner_char = spinner_row
            && (!side_by_side || cells_row.get(col).copied().flatten().is_none())
            && !ch.is_whitespace();
        let slice = cells_row.get(col).copied().flatten();

        let color = if is_spinner_char && slice.is_none() {
            Some(SPINNER_COLOR)
        } else {
            slice.map(palette::color_for_slice)
        };

        if !started || color != current {
            match color {
                Some(c) => execute!(stdout, SetForegroundColor(c))?,
                None => execute!(stdout, ResetColor)?,
            }
            current = color;
            started = true;
        }
        write!(stdout, "{ch}")?;
    }
    execute!(stdout, ResetColor)?;
    Ok(())
}

fn draw_header(stdout: &mut Stdout, current_folder: &Path) -> io::Result<()> {
    execute!(
        stdout,
        MoveTo(0, 0),
        SetAttribute(Attribute::Bold),
        SetForegroundColor(HEADER_COLOR)
    )?;
    write!(stdout, "📁 {}", current_folder.display())?;
    execute!(stdout, SetAttribute(Attribute::Reset), ResetColor)?;
    Ok(())
}

fn draw_view(
    stdout: &mut Stdout,
    current_folder: &Path,
    view: &RenderedView,
    status: &str,
) -> io::Result<()> {
    execute!(stdout, Clear(ClearType::All))?;
    draw_header(stdout, current_folder)?;

    let body_lines: Vec<&str> = view.display.lines().collect();
    for (i, line) in body_lines.iter().enumerate() {
        execute!(stdout, MoveTo(0, HEADER_ROWS + i as u16))?;
        let cells_row: &[Option<usize>] = view
            .targets
            .cells
            .get(i)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        write_colored_line(stdout, line, cells_row)?;
    }

    let footer_row = HEADER_ROWS + body_lines.len() as u16 + 1;
    execute!(stdout, MoveTo(0, footer_row), SetForegroundColor(STATUS_COLOR))?;
    write!(stdout, "{status}")?;
    execute!(stdout, ResetColor)?;
    stdout.flush()
}

/// Walks `line` char-by-char and applies the slice color (from `cells_row`) to
/// each cell, batching consecutive same-color characters into a single write.
fn write_colored_line(
    stdout: &mut Stdout,
    line: &str,
    cells_row: &[Option<usize>],
) -> io::Result<()> {
    let mut current: Option<usize> = None;
    let mut started = false;
    for (col, ch) in line.chars().enumerate() {
        let slice = cells_row.get(col).copied().flatten();
        if !started || slice != current {
            match slice {
                Some(idx) => execute!(stdout, SetForegroundColor(palette::color_for_slice(idx)))?,
                None => execute!(stdout, ResetColor)?,
            }
            current = slice;
            started = true;
        }
        write!(stdout, "{ch}")?;
    }
    execute!(stdout, ResetColor)?;
    Ok(())
}

// --- Mouse coord shifting ---

fn shift_event_into_view(event: Event, view_origin_row: u16) -> Event {
    match event {
        Event::Mouse(mouse) => {
            let new_row = if mouse.row < view_origin_row {
                u16::MAX
            } else {
                mouse.row - view_origin_row
            };
            Event::Mouse(MouseEvent {
                kind: mouse.kind,
                column: mouse.column,
                row: new_row,
                modifiers: mouse.modifiers,
            })
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{
        KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MouseButton, MouseEventKind,
    };

    fn make_key(code: KeyCode) -> Event {
        Event::Key(KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })
    }

    #[test]
    fn shifting_a_mouse_event_subtracts_the_view_origin_from_the_row() {
        let evt = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 5,
            row: 10,
            modifiers: KeyModifiers::NONE,
        });
        let shifted = shift_event_into_view(evt, 3);
        match shifted {
            Event::Mouse(m) => {
                assert_eq!(m.row, 7);
                assert_eq!(m.column, 5);
            }
            _ => panic!("expected mouse event"),
        }
    }

    #[test]
    fn shifting_a_mouse_event_above_the_view_pushes_it_out_of_bounds() {
        let evt = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 5,
            row: 0,
            modifiers: KeyModifiers::NONE,
        });
        let shifted = shift_event_into_view(evt, 3);
        match shifted {
            Event::Mouse(m) => assert_eq!(m.row, u16::MAX),
            _ => panic!("expected mouse event"),
        }
    }

    #[test]
    fn shifting_leaves_non_mouse_events_unchanged() {
        let evt = make_key(KeyCode::Char('q'));
        let shifted = shift_event_into_view(evt.clone(), 3);
        assert_eq!(format!("{:?}", shifted), format!("{:?}", evt));
    }

    #[test]
    fn quit_event_recognises_q_and_escape() {
        assert!(is_quit_event(&make_key(KeyCode::Char('q'))));
        assert!(is_quit_event(&make_key(KeyCode::Char('Q'))));
        assert!(is_quit_event(&make_key(KeyCode::Esc)));
    }

    #[test]
    fn quit_event_does_not_match_other_keys() {
        assert!(!is_quit_event(&make_key(KeyCode::Char('x'))));
        assert!(!is_quit_event(&make_key(KeyCode::Enter)));
    }

    // --- Layout ↔ resize ---

    /// Smoke test: tying the TUI's layout pipeline together — pie length feeds the
    /// layout, layout feeds the renderer, renderer fills the targets — must produce
    /// click-resolvable cells. This guards against regressions where one of those
    /// signals goes missing on resize.
    #[test]
    fn rendering_with_a_computed_layout_yields_clickable_targets_for_each_slice() {
        let pie = vec![
            DiskEntry::folder("Documents", 100),
            DiskEntry::folder("Videos", 50),
            DiskEntry::folder("Photos", 25),
        ];
        let legend_w = renderer::legend_width(&pie);
        let layout = layout::compute_layout(120, 40, pie.len(), legend_w);
        let view = renderer::render_with_layout(&pie, &layout);

        let mut seen = std::collections::HashSet::new();
        for row in &view.targets.cells {
            for cell in row {
                if let Some(idx) = cell {
                    seen.insert(*idx);
                }
            }
        }
        for i in 0..pie.len() {
            assert!(
                seen.contains(&i),
                "slice {} should be clickable in computed layout",
                i
            );
        }
    }

    // --- Skeleton ---

    #[test]
    fn prepare_skeleton_sorts_alphabetically_so_the_legend_is_predictable_during_scan() {
        let raw = vec![
            DiskEntry::folder("Videos", 0),
            DiskEntry::folder("Documents", 0),
            DiskEntry::file("readme.txt", 0),
        ];
        let prepped = prepare_skeleton(raw);
        let names: Vec<&str> = prepped.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["Documents", "Videos", "readme.txt"]);
    }

    #[test]
    fn prepare_skeleton_caps_at_max_slices_with_a_tail_bucket_recording_what_was_dropped() {
        let raw: Vec<DiskEntry> = (0..MAX_SLICES + 5)
            .map(|i| DiskEntry::folder(format!("entry_{:02}", i), 0))
            .collect();
        let prepped = prepare_skeleton(raw);
        // MAX_SLICES kept entries + 1 trailing bucket = MAX_SLICES + 1.
        assert_eq!(prepped.len(), MAX_SLICES + 1);
        let last = prepped.last().unwrap();
        assert!(
            last.name.contains("autres"),
            "tail bucket should mention dropped entries, got {:?}",
            last.name
        );
        assert!(!last.is_drillable(), "tail bucket must not be drillable");
    }

    #[test]
    fn merge_skeleton_keeps_pending_for_names_not_yet_completed() {
        let skeleton = vec![
            DiskEntry::folder("Documents", 0),
            DiskEntry::folder("Videos", 0),
        ];
        let completed: HashMap<String, u64> = HashMap::new();
        let items = merge_skeleton_with_completed(&skeleton, &completed);
        assert!(items.iter().all(|i| i.size.is_none()));
    }

    #[test]
    fn merge_skeleton_fills_in_sizes_for_completed_top_level_children() {
        let skeleton = vec![
            DiskEntry::folder("Documents", 0),
            DiskEntry::folder("Videos", 0),
            DiskEntry::folder("Photos", 0),
        ];
        let mut completed: HashMap<String, u64> = HashMap::new();
        completed.insert("Documents".into(), 1024);
        completed.insert("Photos".into(), 9999);

        let items = merge_skeleton_with_completed(&skeleton, &completed);

        assert_eq!(items[0].name, "Documents");
        assert_eq!(items[0].size, Some(1024));
        assert_eq!(items[1].name, "Videos");
        assert_eq!(items[1].size, None);
        assert_eq!(items[2].name, "Photos");
        assert_eq!(items[2].size, Some(9999));
    }

    #[test]
    fn truncate_to_chars_returns_short_strings_unchanged() {
        assert_eq!(truncate_to_chars("hello", 80), "hello");
    }

    #[test]
    fn truncate_to_chars_caps_long_strings_at_the_requested_width() {
        let long = "C:\\Users\\Adam\\some\\very\\deep\\folder\\path\\going\\on\\forever";
        let result = truncate_to_chars(long, 20);
        assert_eq!(result.chars().count(), 20);
        assert!(long.starts_with(&result));
    }

    #[test]
    fn truncate_to_chars_is_unicode_safe_and_does_not_split_multi_byte_characters() {
        // 5 visible chars: "📁foo→" — would explode under byte slicing.
        let s = "📁foo→bar";
        let result = truncate_to_chars(s, 5);
        assert_eq!(result.chars().count(), 5);
    }

    #[test]
    fn prepare_skeleton_under_max_slices_does_not_invent_a_bucket() {
        let raw = vec![
            DiskEntry::folder("Documents", 0),
            DiskEntry::folder("Videos", 0),
        ];
        let prepped = prepare_skeleton(raw);
        assert_eq!(prepped.len(), 2);
        for entry in &prepped {
            assert!(!entry.name.contains("autres"));
        }
    }

    #[test]
    fn resizing_the_terminal_recomputes_the_pie_radius() {
        let pie = vec![DiskEntry::folder("Only", 100)];
        let legend_w = renderer::legend_width(&pie);
        let small = layout::compute_layout(80, 24, pie.len(), legend_w);
        let large = layout::compute_layout(200, 60, pie.len(), legend_w);
        let small_view = renderer::render_with_layout(&pie, &small);
        let large_view = renderer::render_with_layout(&pie, &large);
        assert!(
            large_view.display.lines().count() > small_view.display.lines().count(),
            "larger terminal should produce a taller render"
        );
    }
}
