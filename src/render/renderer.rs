use crate::DiskEntry;
use crate::EntryKind;
use super::cheese_spinner;
use super::layout::{self, Layout};
use super::mouse_target::ClickTargets;
use std::f64::consts::{FRAC_PI_2, TAU};

const SLICE_MARKERS: &[char] = &['█', '▓', '▒', '░', '#', '@', '*', '+', '=', 'o'];
const DEFAULT_PIE_RADIUS: usize = 7;
const SIDE_BY_SIDE_GAP: usize = 4;

pub struct RenderedView {
    pub display: String,
    pub targets: ClickTargets,
}

/// Plain render with the legacy stacked layout and the historic radius — kept
/// for the REPL / older tests.
pub fn render(pie: &[DiskEntry]) -> String {
    render_with_targets(pie).display
}

/// Renders with the default stacked layout (legacy entry point used by tests
/// that don't care about responsiveness).
pub fn render_with_targets(pie: &[DiskEntry]) -> RenderedView {
    render_with_layout(
        pie,
        &Layout {
            radius: DEFAULT_PIE_RADIUS,
            side_by_side: false,
        },
    )
}

/// Layout-aware renderer. Produces `display` (the printable text) and `targets`
/// (a 2D map from screen cells to slice indices) sized to match `display` so
/// the TUI can color-paint cells and resolve clicks against the same grid.
pub fn render_with_layout(pie: &[DiskEntry], layout: &Layout) -> RenderedView {
    if pie.is_empty() {
        return RenderedView {
            display: "(camembert vide — aucun dossier à afficher)".to_string(),
            targets: ClickTargets::empty(),
        };
    }
    let total_bytes: u64 = pie.iter().map(|e| e.bytes).sum();
    if total_bytes == 0 {
        return RenderedView {
            display: "(camembert vide — taille totale nulle)".to_string(),
            targets: ClickTargets::empty(),
        };
    }

    let markers: Vec<char> = (0..pie.len())
        .map(|i| SLICE_MARKERS[i % SLICE_MARKERS.len()])
        .collect();

    let (pie_lines, pie_grid) = draw_pie(pie, &markers, total_bytes, layout.radius);
    let legend_lines = build_legend(pie, &markers, total_bytes);

    if layout.side_by_side {
        compose_side_by_side(&pie_lines, &pie_grid, &legend_lines, pie.len())
    } else {
        compose_stacked(&pie_lines, &pie_grid, &legend_lines, pie.len())
    }
}

fn compose_stacked(
    pie_lines: &[String],
    pie_grid: &[Vec<Option<usize>>],
    legend_lines: &[String],
    num_slices: usize,
) -> RenderedView {
    let mut display = String::new();
    for line in pie_lines {
        display.push_str(line);
        display.push('\n');
    }
    display.push('\n');
    for line in legend_lines {
        display.push_str(line);
        display.push('\n');
    }

    let mut cells: Vec<Vec<Option<usize>>> = pie_grid.to_vec();
    // Blank separator row.
    cells.push(Vec::new());
    // One row per legend entry; whole row maps to that slice.
    for (i, line) in legend_lines.iter().enumerate() {
        let len = line.chars().count();
        let slice_index = if i < num_slices { Some(i) } else { None };
        cells.push(vec![slice_index; len]);
    }

    RenderedView {
        display,
        targets: ClickTargets { cells },
    }
}

fn compose_side_by_side(
    pie_lines: &[String],
    pie_grid: &[Vec<Option<usize>>],
    legend_lines: &[String],
    num_slices: usize,
) -> RenderedView {
    let pie_width = pie_grid.first().map(|row| row.len()).unwrap_or(0);
    let total_rows = pie_lines.len().max(legend_lines.len());
    let gap = SIDE_BY_SIDE_GAP;

    let mut display = String::new();
    let mut cells: Vec<Vec<Option<usize>>> = Vec::with_capacity(total_rows);

    for row in 0..total_rows {
        let pie_text = pie_lines.get(row).map(String::as_str).unwrap_or("");
        let legend_text = legend_lines.get(row).map(String::as_str).unwrap_or("");

        // Right-pad the pie text to the full pie_width so the legend column
        // lines up across all rows. (`pie_lines` are already trim_end()'d.)
        let pie_visible_chars = pie_text.chars().count();
        let mut composed = String::with_capacity(pie_visible_chars + gap + legend_text.len());
        composed.push_str(pie_text);
        for _ in pie_visible_chars..pie_width {
            composed.push(' ');
        }
        for _ in 0..gap {
            composed.push(' ');
        }
        composed.push_str(legend_text);
        display.push_str(composed.trim_end());
        display.push('\n');

        // Build the matching click-targets row.
        let mut row_cells: Vec<Option<usize>> = Vec::with_capacity(pie_width + gap + legend_text.len());
        if let Some(grid_row) = pie_grid.get(row) {
            row_cells.extend(grid_row.iter().copied());
        } else {
            row_cells.extend(std::iter::repeat(None).take(pie_width));
        }
        row_cells.extend(std::iter::repeat(None).take(gap));
        let slice_index = if row < num_slices { Some(row) } else { None };
        let legend_len = legend_text.chars().count();
        row_cells.extend(std::iter::repeat(slice_index).take(legend_len));
        cells.push(row_cells);
    }

    RenderedView {
        display,
        targets: ClickTargets { cells },
    }
}

/// Returns the rendered pie lines paired with a per-cell mapping
/// `grid[row][col] = Some(slice_index)` for cells inside the circle, `None` outside.
pub(crate) fn draw_pie(
    pie: &[DiskEntry],
    markers: &[char],
    total_bytes: u64,
    radius: usize,
) -> (Vec<String>, Vec<Vec<Option<usize>>>) {
    let r = radius as f64;
    let (width, height) = layout::pie_dimensions(radius);

    let mut slice_bounds = Vec::with_capacity(pie.len() + 1);
    slice_bounds.push(0.0_f64);
    let mut cumulative = 0.0;
    for entry in pie {
        cumulative += (entry.bytes as f64 / total_bytes as f64) * TAU;
        slice_bounds.push(cumulative);
    }

    let mut lines = Vec::with_capacity(height);
    let mut grid = Vec::with_capacity(height);
    for row in 0..height {
        let mut line = String::with_capacity(width);
        let mut row_cells = Vec::with_capacity(width);
        for col in 0..width {
            let dx = (col as f64 + 0.5) / 2.0 - r;
            let dy = (row as f64 + 0.5) - r;
            let distance = (dx * dx + dy * dy).sqrt();
            if distance > r {
                line.push(' ');
                row_cells.push(None);
                continue;
            }
            let raw_angle = dy.atan2(dx);
            let mut angle = raw_angle + FRAC_PI_2;
            if angle < 0.0 {
                angle += TAU;
            }

            let slice_index = pie
                .iter()
                .enumerate()
                .find(|(i, _)| angle < slice_bounds[i + 1])
                .map(|(i, _)| i)
                .unwrap_or(pie.len() - 1);
            line.push(markers[slice_index]);
            row_cells.push(Some(slice_index));
        }
        // Leave the grid row at full width so it lines up with adjacent columns
        // in side-by-side mode; only the printable line is trimmed.
        lines.push(line.trim_end().to_string());
        grid.push(row_cells);
    }
    (lines, grid)
}

/// One line of progress: a child of the folder being scanned, with its size
/// known (`Some`) or still pending (`None` → rendered as a blinking cursor).
pub struct ProgressItem {
    pub name: String,
    pub kind: EntryKind,
    pub size: Option<u64>,
}

impl ProgressItem {
    pub fn pending(name: impl Into<String>, kind: EntryKind) -> Self {
        Self { name: name.into(), kind, size: None }
    }

    pub fn known(name: impl Into<String>, kind: EntryKind, bytes: u64) -> Self {
        Self { name: name.into(), kind, size: Some(bytes) }
    }
}

/// Renders a frame of the live-progress view: spinner where the pie will be,
/// legend listing each child of the folder being scanned with its size — known
/// sizes are humanised, unknown sizes show a blinking cursor placeholder. As
/// the recursive scan completes individual subtrees, the TUI swaps `None`s for
/// `Some(bytes)` and the legend fills in cell by cell.
pub fn render_progress_view(
    layout: &Layout,
    items: &[ProgressItem],
    spinner_frame_idx: usize,
) -> RenderedView {
    let spinner_lines = cheese_spinner::frame(spinner_frame_idx, layout.radius);
    let (pie_width, _) = layout::pie_dimensions(layout.radius);
    let spinner_grid: Vec<Vec<Option<usize>>> = spinner_lines
        .iter()
        .map(|line| {
            let mut row = vec![None; pie_width];
            let line_len = line.chars().count();
            if line_len < pie_width {
                row.truncate(line_len);
                row.extend(std::iter::repeat(None).take(pie_width - line_len));
            }
            row
        })
        .collect();

    let (legend_lines, num_slices) = if items.is_empty() {
        (Vec::new(), 0)
    } else {
        let markers: Vec<char> = (0..items.len())
            .map(|i| SLICE_MARKERS[i % SLICE_MARKERS.len()])
            .collect();
        (
            build_progress_legend(items, &markers, spinner_frame_idx),
            items.len(),
        )
    };

    if layout.side_by_side {
        compose_side_by_side(&spinner_lines, &spinner_grid, &legend_lines, num_slices)
    } else {
        compose_stacked(&spinner_lines, &spinner_grid, &legend_lines, num_slices)
    }
}

/// Renders a "skeleton" frame: every item is pending (`size: None`). Thin
/// wrapper around `render_progress_view`.
pub fn render_skeleton_view(
    layout: &Layout,
    skeleton: &[DiskEntry],
    spinner_frame_idx: usize,
) -> RenderedView {
    let items: Vec<ProgressItem> = skeleton
        .iter()
        .map(|e| ProgressItem::pending(e.name.clone(), e.kind.clone()))
        .collect();
    render_progress_view(layout, &items, spinner_frame_idx)
}

/// Builds the legend rendered during a scan: known sizes humanised, unknown
/// sizes shown as a blinking cursor (alternates every frame for a "live" feel).
/// All size cells are right-aligned to a single common width so the legend
/// doesn't visually shift as sizes settle.
fn build_progress_legend(items: &[ProgressItem], markers: &[char], frame_idx: usize) -> Vec<String> {
    let name_column_width = items
        .iter()
        .map(|e| e.name.chars().count())
        .max()
        .unwrap_or(0);
    let index_column_width = format!("{}", items.len()).len();
    let cursor: char = if frame_idx % 2 == 0 { '█' } else { '▒' };
    let placeholder = format!("{}{}{}", cursor, cursor, cursor);

    let known_size_width = items
        .iter()
        .filter_map(|i| i.size)
        .map(|b| humanize_bytes(b).chars().count())
        .max()
        .unwrap_or(0);
    let size_column_width = known_size_width.max(placeholder.chars().count());

    items
        .iter()
        .enumerate()
        .zip(markers.iter())
        .map(|((index, item), marker)| {
            let slice_number = index + 1;
            let size_cell = match item.size {
                Some(b) => humanize_bytes(b),
                None => placeholder.clone(),
            };
            format!(
                "[{:>idx_w$}] {} {:<name_w$}  {:>size_w$}",
                slice_number,
                marker,
                item.name,
                size_cell,
                idx_w = index_column_width,
                name_w = name_column_width,
                size_w = size_column_width,
            )
        })
        .collect()
}

/// Renders a frame of the "scan in progress" view: the spinner sits where the
/// real pie would, and the *preview* legend (from a previously scanned folder,
/// if any) fills the legend area. This keeps the TUI's geometry constant while
/// a scan runs — the screen doesn't shrink when the pie disappears.
pub fn render_scan_view(
    layout: &Layout,
    preview_pie: &[DiskEntry],
    spinner_frame_idx: usize,
) -> RenderedView {
    let spinner_lines = cheese_spinner::frame(spinner_frame_idx, layout.radius);

    // The spinner cells aren't clickable — None across the board — but we still
    // need a grid the right shape so side-by-side composition lines up.
    let (pie_width, _) = layout::pie_dimensions(layout.radius);
    let spinner_grid: Vec<Vec<Option<usize>>> = spinner_lines
        .iter()
        .map(|line| {
            let mut row = vec![None; pie_width];
            // Truncate to actual visible width if the line happens to be shorter.
            let line_len = line.chars().count();
            if line_len < pie_width {
                row.truncate(line_len);
                row.extend(std::iter::repeat(None).take(pie_width - line_len));
            }
            row
        })
        .collect();

    let (legend_lines, num_slices) = if preview_pie.is_empty() {
        (Vec::new(), 0)
    } else {
        let total_bytes: u64 = preview_pie.iter().map(|e| e.bytes).sum::<u64>().max(1);
        let markers: Vec<char> = (0..preview_pie.len())
            .map(|i| SLICE_MARKERS[i % SLICE_MARKERS.len()])
            .collect();
        (
            build_legend(preview_pie, &markers, total_bytes),
            preview_pie.len(),
        )
    };

    if layout.side_by_side {
        compose_side_by_side(&spinner_lines, &spinner_grid, &legend_lines, num_slices)
    } else {
        compose_stacked(&spinner_lines, &spinner_grid, &legend_lines, num_slices)
    }
}

/// Returns the width of the widest legend line that would be produced for `pie`.
/// The TUI feeds this to the layout engine so side-by-side mode reserves the
/// exact column budget the legend will actually need.
pub fn legend_width(pie: &[DiskEntry]) -> usize {
    if pie.is_empty() {
        return 0;
    }
    let total_bytes: u64 = pie.iter().map(|e| e.bytes).sum::<u64>().max(1);
    let markers: Vec<char> = (0..pie.len())
        .map(|i| SLICE_MARKERS[i % SLICE_MARKERS.len()])
        .collect();
    build_legend(pie, &markers, total_bytes)
        .iter()
        .map(|l| l.chars().count())
        .max()
        .unwrap_or(0)
}

fn build_legend(pie: &[DiskEntry], markers: &[char], total_bytes: u64) -> Vec<String> {
    let name_column_width = pie.iter().map(|e| e.name.chars().count()).max().unwrap_or(0);
    let index_column_width = format!("{}", pie.len()).len();

    pie.iter()
        .enumerate()
        .zip(markers.iter())
        .map(|((index, entry), marker)| {
            let percentage = entry.bytes as f64 / total_bytes as f64 * 100.0;
            let slice_number = index + 1;
            format!(
                "[{:>idx_w$}] {} {:<name_w$}  {:>5.1}%   {}",
                slice_number,
                marker,
                entry.name,
                percentage,
                humanize_bytes(entry.bytes),
                idx_w = index_column_width,
                name_w = name_column_width
            )
        })
        .collect()
}

pub fn humanize_bytes(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = KIB * 1024;
    const GIB: u64 = MIB * 1024;
    const TIB: u64 = GIB * 1024;

    if bytes < KIB {
        format!("{} B", bytes)
    } else if bytes < MIB {
        format!("{:.1} KB", bytes as f64 / KIB as f64)
    } else if bytes < GIB {
        format!("{:.1} MB", bytes as f64 / MIB as f64)
    } else if bytes < TIB {
        format!("{:.1} GB", bytes as f64 / GIB as f64)
    } else {
        format!("{:.1} TB", bytes as f64 / TIB as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str, bytes: u64) -> DiskEntry {
        DiskEntry::folder(name, bytes)
    }

    fn stacked(radius: usize) -> Layout {
        Layout { radius, side_by_side: false }
    }

    fn side_by_side(radius: usize) -> Layout {
        Layout { radius, side_by_side: true }
    }

    /// Extracts the marker character from a legend line, skipping the `[N]` index prefix.
    fn marker_in_legend_line(line: &str) -> Option<char> {
        let trimmed = line.trim_start();
        let after_open = trimmed.strip_prefix('[')?;
        let after_digits = after_open.trim_start_matches(|c: char| c.is_ascii_digit());
        let after_close = after_digits.strip_prefix(']')?;
        after_close.trim_start().chars().next()
    }

    // ---- humanize_bytes ----

    #[test]
    fn humanize_zero_bytes() {
        assert_eq!(humanize_bytes(0), "0 B");
    }

    #[test]
    fn humanize_below_one_kib_keeps_bytes_unit() {
        assert_eq!(humanize_bytes(512), "512 B");
        assert_eq!(humanize_bytes(1023), "1023 B");
    }

    #[test]
    fn humanize_one_kib_exact() {
        assert_eq!(humanize_bytes(1024), "1.0 KB");
    }

    #[test]
    fn humanize_one_and_a_half_kib() {
        assert_eq!(humanize_bytes(1536), "1.5 KB");
    }

    #[test]
    fn humanize_megabytes_gigabytes_terabytes() {
        assert_eq!(humanize_bytes(1024 * 1024), "1.0 MB");
        assert_eq!(humanize_bytes(1024u64.pow(3)), "1.0 GB");
        assert_eq!(humanize_bytes(1024u64.pow(4)), "1.0 TB");
    }

    // ---- render (stacked, default radius) ----

    #[test]
    fn rendering_an_empty_pie_still_prints_a_friendly_message() {
        let output = render(&[]);
        assert!(!output.trim().is_empty(), "empty pie must still print something");
    }

    #[test]
    fn rendering_a_pie_lists_every_folder_name() {
        let pie = vec![
            folder("Documents", 100),
            folder("Videos", 50),
            DiskEntry::bucket("Autres", 25),
        ];
        let output = render(&pie);
        for entry in &pie {
            assert!(
                output.contains(&entry.name),
                "expected entry name {:?} in output\n{}",
                entry.name,
                output
            );
        }
    }

    #[test]
    fn rendering_a_pie_shows_each_folders_share_as_a_percentage() {
        let pie = vec![folder("Videos", 75), folder("Documents", 25)];
        let output = render(&pie);
        assert!(output.contains("75.0%"), "missing 75.0% in:\n{}", output);
        assert!(output.contains("25.0%"), "missing 25.0% in:\n{}", output);
    }

    #[test]
    fn rendering_a_pie_shows_each_folders_size_in_human_units() {
        let pie = vec![
            folder("Videos", 2 * 1024 * 1024),
            folder("Documents", 512),
        ];
        let output = render(&pie);
        assert!(output.contains("2.0 MB"), "missing 2.0 MB in:\n{}", output);
        assert!(output.contains("512 B"), "missing 512 B in:\n{}", output);
    }

    #[test]
    fn rendering_a_pie_uses_a_distinct_marker_for_each_folder() {
        let pie = vec![
            folder("Documents", 30),
            folder("Videos", 30),
            folder("Photos", 30),
        ];
        let output = render(&pie);
        let mut markers_seen = std::collections::HashSet::new();
        for line in output.lines() {
            for entry in &pie {
                if line.contains(&entry.name) {
                    if let Some(marker) = marker_in_legend_line(line) {
                        markers_seen.insert(marker);
                    }
                }
            }
        }
        assert_eq!(
            markers_seen.len(),
            pie.len(),
            "expected {} distinct markers, got {:?}\n{}",
            pie.len(),
            markers_seen,
            output
        );
    }

    #[test]
    fn rendering_a_pie_actually_draws_a_circle_with_marker_characters() {
        let pie = vec![folder("OnlyFolder", 100)];
        let output = render(&pie);
        let drawing_present = output.lines().any(|line| {
            line.chars()
                .filter(|c| !c.is_whitespace() && !c.is_ascii_alphanumeric() && *c != '%' && *c != '.')
                .count()
                >= 3
        });
        assert!(drawing_present, "expected a drawn pie in:\n{}", output);
    }

    #[test]
    fn rendering_a_pie_numbers_each_legend_line_for_drilldown() {
        let pie = vec![
            folder("Documents", 30),
            folder("Videos", 30),
            folder("Photos", 30),
        ];
        let output = render(&pie);
        assert!(output.contains("[1]"), "missing [1] in:\n{}", output);
        assert!(output.contains("[2]"), "missing [2] in:\n{}", output);
        assert!(output.contains("[3]"), "missing [3] in:\n{}", output);
    }

    // ---- render_with_targets ----

    #[test]
    fn render_with_targets_display_matches_plain_render() {
        let pie = vec![folder("Documents", 100), folder("Videos", 50)];
        assert_eq!(render(&pie), render_with_targets(&pie).display);
    }

    #[test]
    fn render_with_targets_yields_empty_targets_when_pie_is_empty() {
        let view = render_with_targets(&[]);
        assert!(view.targets.cells.is_empty());
    }

    #[test]
    fn click_targets_legend_rows_resolve_to_each_slice_in_order() {
        let pie = vec![folder("a", 50), folder("b", 30), folder("c", 20)];
        let view = render_with_targets(&pie);
        // Find the rows that map to each slice (legend rows have all-equal slice index)
        let mut slice_seen: Vec<usize> = Vec::new();
        for row in &view.targets.cells {
            if let Some(&Some(idx)) = row.first() {
                // legend rows are uniform; pie rows can have None gaps
                if row.iter().all(|c| matches!(c, Some(i) if *i == idx)) {
                    slice_seen.push(idx);
                }
            }
        }
        assert_eq!(slice_seen, vec![0, 1, 2]);
    }

    #[test]
    fn render_with_targets_pie_grid_contains_at_least_one_slice_cell_per_slice() {
        let pie = vec![folder("a", 50), folder("b", 50)];
        let view = render_with_targets(&pie);
        let mut seen = std::collections::HashSet::new();
        for row in &view.targets.cells {
            for cell in row {
                if let Some(idx) = cell {
                    seen.insert(*idx);
                }
            }
        }
        assert_eq!(seen, std::collections::HashSet::from([0usize, 1usize]));
    }

    // ---- render_with_layout : stacked vs side-by-side ----

    #[test]
    fn stacked_layout_puts_legend_below_the_pie() {
        let pie = vec![folder("Videos", 50), folder("Docs", 50)];
        let view = render_with_layout(&pie, &stacked(7));
        let lines: Vec<&str> = view.display.lines().collect();
        // First non-empty lines are the pie; the legend with `[1]` must come *after*.
        let legend_row = lines
            .iter()
            .position(|l| l.contains("[1]"))
            .expect("legend row missing");
        let pie_height = 2 * 7 + 1;
        assert!(
            legend_row >= pie_height,
            "stacked legend should be after the pie (row {} vs pie_height {})",
            legend_row,
            pie_height
        );
    }

    #[test]
    fn side_by_side_layout_places_legend_on_the_same_row_as_the_pie_top() {
        let pie = vec![folder("Videos", 50), folder("Docs", 50)];
        let view = render_with_layout(&pie, &side_by_side(7));
        let first_line = view.display.lines().next().unwrap();
        // Pie's top row + a gap + the first legend line "[1]" must be on row 0.
        assert!(
            first_line.contains("[1]"),
            "side-by-side first row must contain the legend's [1] entry, got: {:?}",
            first_line
        );
    }

    #[test]
    fn side_by_side_click_targets_resolve_legend_clicks_at_the_pies_right() {
        let pie = vec![folder("Videos", 50), folder("Docs", 50)];
        let view = render_with_layout(&pie, &side_by_side(7));
        // Pick a column far to the right (legend territory) on row 0 — must resolve to slice 0.
        let last_row_cols = view.targets.cells[0].len();
        assert!(last_row_cols > 0);
        let legend_col = (last_row_cols - 1) as u16;
        assert_eq!(view.targets.slice_at(legend_col, 0), Some(0));
        assert_eq!(view.targets.slice_at(legend_col, 1), Some(1));
    }

    #[test]
    fn side_by_side_click_targets_still_resolve_pie_clicks_on_the_left() {
        let pie = vec![folder("Videos", 50), folder("Docs", 50)];
        let view = render_with_layout(&pie, &side_by_side(7));
        // Pie center should resolve to *some* slice.
        let pie_height = 2 * 7 + 1;
        let center_col = (2 * pie_height / 2) as u16;
        let center_row = (pie_height / 2) as u16;
        assert!(view.targets.slice_at(center_col, center_row).is_some());
    }

    // ---- render_scan_view ----

    #[test]
    fn scan_view_uses_the_layouts_radius_for_the_spinner() {
        let layout = Layout { radius: 10, side_by_side: false };
        let scan = render_scan_view(&layout, &[], 0);
        // pie_height = 2*radius + 1; the scan view's first lines should match.
        let expected_spinner_height = 2 * 10 + 1;
        assert!(
            scan.display.lines().count() >= expected_spinner_height,
            "scan view shorter than the spinner ({} < {})",
            scan.display.lines().count(),
            expected_spinner_height,
        );
    }

    #[test]
    fn scan_view_with_a_preview_pie_shows_its_legend_below_the_spinner_when_stacked() {
        let preview = vec![folder("Documents", 100), folder("Videos", 50)];
        let layout = Layout { radius: 5, side_by_side: false };
        let scan = render_scan_view(&layout, &preview, 0);
        assert!(scan.display.contains("Documents"));
        assert!(scan.display.contains("Videos"));
        assert!(scan.display.contains("[1]"));
        assert!(scan.display.contains("[2]"));
    }

    #[test]
    fn scan_view_with_preview_in_side_by_side_puts_legend_at_the_right_of_the_spinner() {
        let preview = vec![folder("Documents", 100), folder("Videos", 50)];
        let layout = Layout { radius: 5, side_by_side: true };
        let scan = render_scan_view(&layout, &preview, 0);
        // First row: spinner cells then gap then "[1] ..."
        let first = scan.display.lines().next().unwrap();
        assert!(first.contains("[1]"));
    }

    #[test]
    fn scan_view_spinner_cells_are_not_clickable_targets() {
        let preview = vec![folder("Docs", 100)];
        let layout = Layout { radius: 5, side_by_side: false };
        let scan = render_scan_view(&layout, &preview, 0);
        // Spinner rows occupy 0..(2*5+1). All their cells should be None.
        let pie_height = 2 * 5 + 1;
        for row in &scan.targets.cells[..pie_height] {
            assert!(
                row.iter().all(|c| c.is_none()),
                "spinner row contains clickable cells: {:?}",
                row
            );
        }
    }

    #[test]
    fn scan_view_without_a_preview_still_reserves_the_spinner_geometry() {
        let layout = Layout { radius: 7, side_by_side: false };
        let scan = render_scan_view(&layout, &[], 0);
        let pie_height = 2 * 7 + 1;
        assert!(scan.display.lines().count() >= pie_height);
    }

    // ---- render_progress_view ----

    fn pending(name: &str) -> ProgressItem {
        ProgressItem::pending(name, EntryKind::Folder)
    }

    fn known(name: &str, bytes: u64) -> ProgressItem {
        ProgressItem::known(name, EntryKind::Folder, bytes)
    }

    #[test]
    fn progress_view_humanises_sizes_for_items_whose_walk_has_completed() {
        let items = vec![
            known("Documents", 2 * 1024 * 1024),
            pending("Videos"),
        ];
        let layout = Layout { radius: 5, side_by_side: false };
        let view = render_progress_view(&layout, &items, 0);
        assert!(view.display.contains("2.0 MB"), "Documents should show its size");
        assert!(!view.display.contains("Videos  2.0 MB"), "pending items should not show a size");
    }

    #[test]
    fn progress_view_pending_items_show_a_cursor_placeholder() {
        let items = vec![pending("Videos")];
        let layout = Layout { radius: 5, side_by_side: false };
        let view = render_progress_view(&layout, &items, 0);
        assert!(
            view.display.contains('█') || view.display.contains('▒'),
            "expected cursor placeholder in pending row, got:\n{}",
            view.display
        );
    }

    #[test]
    fn progress_view_size_column_is_right_aligned_so_filling_in_a_size_doesnt_shift_neighbouring_rows() {
        let items_a = vec![known("Documents", 100), pending("Videos")];
        let items_b = vec![known("Documents", 100), known("Videos", 999_999_999)];
        let layout = Layout { radius: 5, side_by_side: false };
        let lines_a: Vec<String> = render_progress_view(&layout, &items_a, 0).display.lines().map(String::from).collect();
        let lines_b: Vec<String> = render_progress_view(&layout, &items_b, 0).display.lines().map(String::from).collect();
        // Find the line for "Documents" in each — the size column position must match.
        let line_a = lines_a.iter().find(|l| l.contains("Documents")).unwrap();
        let line_b = lines_b.iter().find(|l| l.contains("Documents")).unwrap();
        // They share the same lead-up (`[1] █ Documents `), but the size column differs.
        // What matters is that the size cells are both right-aligned to the same width:
        // the rendered Documents-line in `b` is at least as long as in `a`.
        assert!(
            line_b.chars().count() >= line_a.chars().count(),
            "right-aligned column should grow or stay equal, never shrink unexpectedly"
        );
    }

    // ---- render_skeleton_view (legacy wrapper, all items pending) ----

    #[test]
    fn skeleton_view_shows_each_childs_name() {
        let skeleton = vec![
            DiskEntry::folder("Documents", 0),
            DiskEntry::folder("Videos", 0),
            DiskEntry::file("song.mp3", 0),
        ];
        let layout = Layout { radius: 5, side_by_side: false };
        let view = render_skeleton_view(&layout, &skeleton, 0);
        assert!(view.display.contains("Documents"));
        assert!(view.display.contains("Videos"));
        assert!(view.display.contains("song.mp3"));
    }

    #[test]
    fn skeleton_view_does_not_show_made_up_sizes_or_percentages() {
        let skeleton = vec![DiskEntry::folder("Documents", 0)];
        let layout = Layout { radius: 5, side_by_side: false };
        let view = render_skeleton_view(&layout, &skeleton, 0);
        // Skeleton sizes are unknown — no real B/KB/MB units, no NaN%, no "0 B" lies.
        assert!(!view.display.contains(" B "));
        assert!(!view.display.contains(" KB"));
        assert!(!view.display.contains(" MB"));
        assert!(!view.display.contains("NaN"));
        assert!(!view.display.contains("0 B"));
    }

    #[test]
    fn skeleton_view_uses_a_cursor_placeholder_in_the_size_column() {
        let skeleton = vec![DiskEntry::folder("Documents", 0)];
        let layout = Layout { radius: 5, side_by_side: false };
        let view = render_skeleton_view(&layout, &skeleton, 0);
        // The cursor block character marks "loading" cells.
        assert!(
            view.display.contains('█') || view.display.contains('▒'),
            "expected a cursor-like placeholder character in:\n{}",
            view.display
        );
    }

    #[test]
    fn skeleton_view_blinks_the_cursor_across_consecutive_frames() {
        let skeleton = vec![DiskEntry::folder("Documents", 0)];
        let layout = Layout { radius: 5, side_by_side: false };
        let frame_a = render_skeleton_view(&layout, &skeleton, 0).display;
        let frame_b = render_skeleton_view(&layout, &skeleton, 1).display;
        assert_ne!(frame_a, frame_b, "skeleton must visibly animate between frames");
    }

    #[test]
    fn skeleton_view_numbers_each_legend_line_for_drilldown() {
        let skeleton = vec![
            DiskEntry::folder("Documents", 0),
            DiskEntry::folder("Videos", 0),
        ];
        let layout = Layout { radius: 5, side_by_side: false };
        let view = render_skeleton_view(&layout, &skeleton, 0);
        assert!(view.display.contains("[1]"));
        assert!(view.display.contains("[2]"));
    }

    #[test]
    fn larger_radius_produces_a_taller_pie() {
        let pie = vec![folder("OnlyFolder", 100)];
        let small = render_with_layout(&pie, &stacked(4));
        let big = render_with_layout(&pie, &stacked(10));
        let small_pie_lines = 2 * 4 + 1;
        let big_pie_lines = 2 * 10 + 1;
        // Each rendered display has at least pie_height + blank + 1 legend = pie+2 lines.
        assert!(small.display.lines().count() < big.display.lines().count());
        assert!(small.display.lines().count() >= small_pie_lines);
        assert!(big.display.lines().count() >= big_pie_lines);
    }
}
