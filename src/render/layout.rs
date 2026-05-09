/// Geometry chosen for a given terminal size.
///
/// `radius` controls the pie's size; pie height is `2 * radius + 1` rows and pie
/// width is `2 * (2 * radius + 1)` columns (terminal cells are roughly twice as
/// tall as wide). When `side_by_side` is true, the legend sits to the right of
/// the pie; otherwise it sits below.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub radius: usize,
    pub side_by_side: bool,
}

/// Smallest radius that still draws a recognisable pie.
pub const MIN_RADIUS: usize = 4;
/// Cap on radius so absurdly large terminals don't produce hard-to-read output.
/// Pie height at the cap = 2*60+1 = 121 rows, pie width = 242 columns — already
/// larger than any sane terminal; the layout will pick a smaller radius first.
pub const MAX_RADIUS: usize = 60;

const HEADER_ROWS: usize = 1;
const FOOTER_ROWS: usize = 2;
const LEGEND_BLANK_LINE: usize = 1;
const SIDE_BY_SIDE_GAP: usize = 4;

/// Returns `(width, height)` of the pie for a given `radius`.
pub fn pie_dimensions(radius: usize) -> (usize, usize) {
    let height = 2 * radius + 1;
    let width = 2 * height;
    (width, height)
}

/// Picks a `Layout` that fills as much of the terminal as possible.
///
/// `legend_width` is the *actual* widest line of the legend (in characters),
/// measured from the real pie data — not an estimate. This matters when folder
/// names are long: a guess that's too small overflows; a guess that's too large
/// shrinks the pie unnecessarily.
pub fn compute_layout(
    term_width: u16,
    term_height: u16,
    num_slices: usize,
    legend_width: usize,
) -> Layout {
    let term_width = term_width as usize;
    let term_height = term_height as usize;
    let n = num_slices.max(1);

    if let Some(r) = best_radius_side_by_side(term_width, term_height, n, legend_width) {
        return Layout { radius: r, side_by_side: true };
    }
    Layout {
        radius: best_radius_stacked(term_height, n),
        side_by_side: false,
    }
}

fn best_radius_stacked(term_height: usize, n: usize) -> usize {
    // Total rows needed: HEADER + pie_height + LEGEND_BLANK + n + FOOTER
    // pie_height = 2r + 1
    let available = term_height
        .saturating_sub(HEADER_ROWS + LEGEND_BLANK_LINE + n + FOOTER_ROWS);
    // 2r + 1 <= available  =>  r <= (available - 1) / 2
    let r = available.saturating_sub(1) / 2;
    r.clamp(MIN_RADIUS, MAX_RADIUS)
}

fn best_radius_side_by_side(
    term_width: usize,
    term_height: usize,
    n: usize,
    legend_width: usize,
) -> Option<usize> {
    // Vertical: HEADER + max(pie_height, n) + FOOTER <= term_height
    //   pie_height = 2r + 1  so  2r + 1 <= term_height - HEADER - FOOTER
    //   AND legend has to fit: n <= term_height - HEADER - FOOTER
    let v_avail = term_height.saturating_sub(HEADER_ROWS + FOOTER_ROWS);
    if v_avail < n {
        // Legend itself doesn't fit on this height — fall back to stacked which
        // gives the user a chance to scroll. (Unlikely on real terminals.)
        return None;
    }
    let r_v = v_avail.saturating_sub(1) / 2;

    // Horizontal: pie_width + GAP + legend_width <= term_width
    //   pie_width = 4r + 2  so  4r + 2 <= term_width - GAP - legend_width
    let h_budget = term_width.saturating_sub(SIDE_BY_SIDE_GAP + legend_width + 2);
    let r_h = h_budget / 4;

    let r = r_v.min(r_h);
    if r < MIN_RADIUS {
        return None;
    }
    Some(r.min(MAX_RADIUS))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Realistic legend width for typical pies in tests; mirrors what
    /// `renderer::legend_width` would return for a small disk root.
    const TYPICAL_LEGEND_WIDTH: usize = 32;

    #[test]
    fn pie_dimensions_match_renderers_geometry() {
        // height = 2r+1, width = 2*height
        assert_eq!(pie_dimensions(7), (30, 15));
        assert_eq!(pie_dimensions(4), (18, 9));
    }

    #[test]
    fn very_small_terminal_clamps_to_min_radius_in_stacked_mode() {
        let layout = compute_layout(30, 12, 4, TYPICAL_LEGEND_WIDTH);
        assert!(!layout.side_by_side, "narrow terminal should stack");
        assert_eq!(layout.radius, MIN_RADIUS);
    }

    #[test]
    fn standard_80_by_24_terminal_with_few_slices_goes_side_by_side() {
        // 80 wide is enough for a small pie + legend; 24 tall handles header/footer.
        let layout = compute_layout(80, 24, 4, TYPICAL_LEGEND_WIDTH);
        assert!(layout.side_by_side, "80x24 with 4 slices should use side-by-side");
        assert!(layout.radius >= MIN_RADIUS);
    }

    #[test]
    fn very_wide_terminal_grows_the_pie_radius() {
        let small = compute_layout(80, 30, 4, TYPICAL_LEGEND_WIDTH);
        let large = compute_layout(200, 60, 4, TYPICAL_LEGEND_WIDTH);
        assert!(
            large.radius > small.radius,
            "wider/taller terminal should grow the pie (got {} vs {})",
            large.radius,
            small.radius
        );
    }

    #[test]
    fn radius_grows_well_past_the_old_cap_on_huge_terminals() {
        // The old cap was 15 — this guarantees we no longer stop there on a
        // 4K-ish terminal with room to spare.
        let layout = compute_layout(300, 100, 4, TYPICAL_LEGEND_WIDTH);
        assert!(
            layout.radius > 15,
            "huge terminal should grow past the old 15 cap, got {}",
            layout.radius
        );
    }

    #[test]
    fn radius_never_exceeds_max_radius_on_absurdly_huge_terminals() {
        let layout = compute_layout(2000, 1000, 4, TYPICAL_LEGEND_WIDTH);
        assert!(layout.radius <= MAX_RADIUS);
    }

    #[test]
    fn tall_narrow_terminal_stacks_but_uses_a_bigger_radius_than_minimum() {
        // 50 wide isn't enough for side-by-side, but 50 tall lets the pie grow.
        let layout = compute_layout(50, 50, 4, TYPICAL_LEGEND_WIDTH);
        assert!(!layout.side_by_side);
        assert!(
            layout.radius > MIN_RADIUS,
            "tall stacked terminal should use radius > MIN_RADIUS, got {}",
            layout.radius
        );
    }

    #[test]
    fn many_slices_in_stacked_mode_shrinks_the_pie_to_make_room_for_the_legend() {
        let few = compute_layout(50, 30, 2, TYPICAL_LEGEND_WIDTH);
        let many = compute_layout(50, 30, 9, TYPICAL_LEGEND_WIDTH);
        assert!(
            few.radius >= many.radius,
            "more legend lines should leave less room for the pie ({} vs {})",
            few.radius,
            many.radius,
        );
    }

    #[test]
    fn zero_slices_does_not_panic_or_underflow() {
        let _ = compute_layout(80, 24, 0, TYPICAL_LEGEND_WIDTH);
    }

    #[test]
    fn side_by_side_geometry_actually_fits_in_the_reported_terminal() {
        let layout = compute_layout(120, 40, 6, TYPICAL_LEGEND_WIDTH);
        assert!(layout.side_by_side);
        let (pie_w, pie_h) = pie_dimensions(layout.radius);
        assert!(pie_w + SIDE_BY_SIDE_GAP + TYPICAL_LEGEND_WIDTH <= 120);
        assert!(HEADER_ROWS + pie_h + FOOTER_ROWS <= 40);
    }

    #[test]
    fn a_wider_legend_eats_into_the_pies_horizontal_budget() {
        // Same terminal, longer folder names → the pie has to be narrower.
        let with_short_names = compute_layout(120, 40, 5, 20);
        let with_long_names = compute_layout(120, 40, 5, 60);
        assert!(
            with_long_names.radius <= with_short_names.radius,
            "longer legend should leave less room for the pie ({} vs {})",
            with_long_names.radius,
            with_short_names.radius,
        );
    }
}
