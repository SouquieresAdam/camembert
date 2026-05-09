/// Maps a screen cell `(column, row)` (relative to the top-left of the rendered view)
/// to the index of the pie slice it belongs to, if any.
///
/// Backed by a uniform 2D grid: `cells[row][col] = Some(slice_index)` for cells
/// that should react to clicks (pie cells, legend rows), `None` otherwise. This
/// is layout-agnostic — stacked and side-by-side layouts both populate the same
/// grid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClickTargets {
    pub cells: Vec<Vec<Option<usize>>>,
}

impl ClickTargets {
    pub fn empty() -> Self {
        Self { cells: Vec::new() }
    }

    pub fn slice_at(&self, column: u16, row: u16) -> Option<usize> {
        self.cells
            .get(row as usize)?
            .get(column as usize)
            .copied()
            .flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a tiny 3x3 pie grid: top half slice 0, bottom half slice 1,
    /// followed by a blank line and one legend row per slice.
    fn tiny_targets_stacked() -> ClickTargets {
        ClickTargets {
            cells: vec![
                // pie rows
                vec![None,    Some(0), None   ],
                vec![Some(0), Some(0), Some(1)],
                vec![None,    Some(1), None   ],
                // blank
                vec![None, None, None],
                // legend rows (column doesn't matter, whole row maps to slice)
                vec![Some(0), Some(0), Some(0)],
                vec![Some(1), Some(1), Some(1)],
            ],
        }
    }

    #[test]
    fn empty_targets_resolve_no_clicks() {
        let targets = ClickTargets::empty();
        assert_eq!(targets.slice_at(0, 0), None);
        assert_eq!(targets.slice_at(50, 50), None);
    }

    #[test]
    fn clicking_inside_a_pie_cell_returns_its_slice_index() {
        let targets = tiny_targets_stacked();
        assert_eq!(targets.slice_at(1, 0), Some(0));
        assert_eq!(targets.slice_at(1, 2), Some(1));
        assert_eq!(targets.slice_at(2, 1), Some(1));
    }

    #[test]
    fn clicking_outside_the_pie_circle_returns_none() {
        let targets = tiny_targets_stacked();
        assert_eq!(targets.slice_at(0, 0), None);
        assert_eq!(targets.slice_at(2, 0), None);
        assert_eq!(targets.slice_at(0, 2), None);
        assert_eq!(targets.slice_at(2, 2), None);
    }

    #[test]
    fn clicking_past_the_grids_columns_returns_none() {
        let targets = tiny_targets_stacked();
        assert_eq!(targets.slice_at(99, 1), None);
    }

    #[test]
    fn clicking_on_the_blank_line_between_pie_and_legend_returns_none() {
        let targets = tiny_targets_stacked();
        assert_eq!(targets.slice_at(0, 3), None);
        assert_eq!(targets.slice_at(2, 3), None);
    }

    #[test]
    fn clicking_a_legend_line_returns_that_slice_index() {
        let targets = tiny_targets_stacked();
        assert_eq!(targets.slice_at(0, 4), Some(0));
        assert_eq!(targets.slice_at(0, 5), Some(1));
        assert_eq!(targets.slice_at(2, 4), Some(0));
    }

    #[test]
    fn clicking_below_all_legend_lines_returns_none() {
        let targets = tiny_targets_stacked();
        assert_eq!(targets.slice_at(0, 6), None);
        assert_eq!(targets.slice_at(0, 100), None);
    }

    #[test]
    fn side_by_side_grid_supports_both_pie_and_legend_at_same_rows() {
        // Pie on the left, legend on the right, sharing rows.
        let targets = ClickTargets {
            cells: vec![
                vec![Some(0), Some(0), None, None, Some(0), Some(0)],
                vec![Some(0), Some(1), None, None, Some(1), Some(1)],
                vec![Some(1), Some(1), None, None, None,    None   ],
            ],
        };
        // Pie clicks
        assert_eq!(targets.slice_at(0, 0), Some(0));
        assert_eq!(targets.slice_at(0, 2), Some(1));
        // Legend clicks (right of the gap)
        assert_eq!(targets.slice_at(4, 0), Some(0));
        assert_eq!(targets.slice_at(5, 1), Some(1));
        // Gap between pie and legend
        assert_eq!(targets.slice_at(2, 0), None);
    }
}
