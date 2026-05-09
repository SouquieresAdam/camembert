use crossterm::style::Color;

/// Palette of visually distinct colors for pie slices.
/// Designed to be readable on both dark and light terminals; the last entry is
/// reserved-feeling (DarkGrey) to suit the synthetic « Autres » bucket which
/// always lands at the end of the slice list.
const SLICE_PALETTE: &[Color] = &[
    Color::Red,
    Color::Yellow,
    Color::Green,
    Color::Cyan,
    Color::Blue,
    Color::Magenta,
    Color::DarkYellow,
    Color::DarkGreen,
    Color::DarkCyan,
    Color::DarkGrey,
];

/// Returns a color for the slice at `slice_index`. Cycles through the palette
/// if `slice_index` exceeds its length.
pub fn color_for_slice(slice_index: usize) -> Color {
    SLICE_PALETTE[slice_index % SLICE_PALETTE.len()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn first_few_slices_each_get_a_distinct_color() {
        let colors: HashSet<_> = (0..5).map(color_for_slice).collect();
        assert_eq!(colors.len(), 5, "expected 5 distinct colors for slices 0..5");
    }

    #[test]
    fn the_palette_cycles_when_there_are_more_slices_than_colors() {
        let palette_len = SLICE_PALETTE.len();
        assert_eq!(color_for_slice(0), color_for_slice(palette_len));
        assert_eq!(color_for_slice(3), color_for_slice(palette_len + 3));
    }

    #[test]
    fn no_color_in_the_palette_is_plain_black_or_white() {
        // Plain black/white would be unreadable on the matching terminal background.
        for &c in SLICE_PALETTE {
            assert!(
                !matches!(c, Color::Black | Color::White | Color::Reset),
                "palette must not contain {:?}",
                c
            );
        }
    }

    #[test]
    fn palette_offers_at_least_eight_colors_to_match_max_slices() {
        // tui::MAX_SLICES is 8; palette must cover at least that without immediate cycling.
        assert!(
            SLICE_PALETTE.len() >= 8,
            "palette has only {} colors, want >= 8",
            SLICE_PALETTE.len()
        );
    }
}
