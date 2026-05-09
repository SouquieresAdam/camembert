//! Renders a frame of the "cheese spinner" — a Pac-Man-style camembert with a
//! single missing wedge that rotates around the wheel to suggest progress.

use std::f64::consts::{FRAC_PI_2, TAU};

pub const FRAMES_PER_TURN: usize = 12;
pub const FRAME_DURATION_MS: u64 = 80;

const FILLED_CHAR: char = '█';
const EMPTY_CHAR: char = ' ';

/// Returns the spinner frame as an owned set of equally-wide lines.
/// `frame_index` cycles through `0..FRAMES_PER_TURN` (modulo) to spin clockwise.
pub fn frame(frame_index: usize, radius: usize) -> Vec<String> {
    let missing_slice = frame_index % FRAMES_PER_TURN;
    let slice_angle = TAU / FRAMES_PER_TURN as f64;
    let missing_start = missing_slice as f64 * slice_angle;
    let missing_end = missing_start + slice_angle;

    let r = radius as f64;
    let height = radius * 2 + 1;
    let width = height * 2; // chars are roughly twice as tall as wide

    let mut lines = Vec::with_capacity(height);
    for row in 0..height {
        let mut line = String::with_capacity(width);
        for col in 0..width {
            let dx = (col as f64 + 0.5) / 2.0 - r;
            let dy = (row as f64 + 0.5) - r;
            let distance = (dx * dx + dy * dy).sqrt();
            if distance > r {
                line.push(EMPTY_CHAR);
                continue;
            }
            let raw_angle = dy.atan2(dx);
            let mut angle = raw_angle + FRAC_PI_2;
            if angle < 0.0 {
                angle += TAU;
            }
            if angle >= missing_start && angle < missing_end {
                line.push(EMPTY_CHAR);
            } else {
                line.push(FILLED_CHAR);
            }
        }
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count_filled(frame: &[String]) -> usize {
        frame.iter().flat_map(|l| l.chars()).filter(|c| *c == FILLED_CHAR).count()
    }

    #[test]
    fn a_frame_has_the_expected_number_of_lines() {
        let lines = frame(0, 4);
        assert_eq!(lines.len(), 4 * 2 + 1);
    }

    #[test]
    fn every_line_has_the_same_width() {
        let lines = frame(3, 5);
        let expected_width = (5 * 2 + 1) * 2;
        for (i, line) in lines.iter().enumerate() {
            assert_eq!(line.chars().count(), expected_width, "line {i} width mismatch");
        }
    }

    #[test]
    fn cells_outside_the_disc_are_blank() {
        let lines = frame(0, 4);
        // A corner cell of the bounding box is outside the inscribed disc.
        let first_cell = lines[0].chars().next().unwrap();
        let last_cell = lines.last().unwrap().chars().last().unwrap();
        assert_eq!(first_cell, EMPTY_CHAR);
        assert_eq!(last_cell, EMPTY_CHAR);
    }

    #[test]
    fn most_of_the_disc_is_filled_for_any_frame() {
        // Removing 1/12 of the wheel should still leave the majority filled.
        for frame_index in 0..FRAMES_PER_TURN {
            let lines = frame(frame_index, 5);
            let filled = count_filled(&lines);
            assert!(
                filled > 30,
                "frame {frame_index} only had {filled} filled cells"
            );
        }
    }

    #[test]
    fn rotating_the_frame_index_changes_the_picture() {
        let frame_a = frame(0, 5);
        let frame_b = frame(3, 5);
        assert_ne!(frame_a, frame_b, "expected rotation between frames");
    }

    #[test]
    fn frame_index_wraps_around_a_full_turn() {
        // After FRAMES_PER_TURN frames we should be back to the same picture.
        let baseline = frame(0, 5);
        let after_full_turn = frame(FRAMES_PER_TURN, 5);
        assert_eq!(baseline, after_full_turn);
    }

    #[test]
    fn the_first_frame_has_its_missing_wedge_at_the_top() {
        // Missing slice 0 occupies the top of the wheel (12 o'clock).
        // The center column near the top should NOT be filled.
        let lines = frame(0, 5);
        let center_col = (5 * 2 + 1); // half of width
        let top_centre_cell = lines[0].chars().nth(center_col).unwrap();
        assert_eq!(
            top_centre_cell, EMPTY_CHAR,
            "frame 0 must have its gap at the top, got:\n{}",
            lines.join("\n")
        );
    }

    #[test]
    fn a_frame_at_six_oclock_has_its_gap_at_the_bottom() {
        // Slice index 6 of 12 is at 6 o'clock (bottom).
        let lines = frame(6, 5);
        let center_col = 5 * 2 + 1;
        let bottom_centre_cell = lines.last().unwrap().chars().nth(center_col).unwrap();
        assert_eq!(
            bottom_centre_cell, EMPTY_CHAR,
            "frame 6 must have its gap at the bottom, got:\n{}",
            lines.join("\n")
        );
    }
}
