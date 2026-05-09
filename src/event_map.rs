use crate::command::Command;
use crate::render::mouse_target::ClickTargets;
use crossterm::event::{Event, KeyCode, MouseButton, MouseEventKind};

/// Translates a `crossterm` event into a `Command`, given the click-target map of the
/// currently displayed view.
///
/// Mouse coordinates in `event` are expected to be **view-relative** — the TUI loop
/// must subtract the view's screen origin before calling this.
pub fn event_to_command(event: &Event, targets: &ClickTargets) -> Option<Command> {
    match event {
        Event::Key(key) => match key.code {
            KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => Some(Command::Quit),
            KeyCode::Char('u') | KeyCode::Char('U') => Some(Command::Up),
            KeyCode::Char('r') | KeyCode::Char('R') => Some(Command::Refresh),
            KeyCode::Char('d') | KeyCode::Char('D') => Some(Command::ChangeDrive),
            KeyCode::Char(c) if ('1'..='9').contains(&c) => {
                let slice_number = (c as u8 - b'0') as usize;
                Some(Command::DrillInto { slice_number })
            }
            _ => None,
        },
        Event::Mouse(mouse) => match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => targets
                .slice_at(mouse.column, mouse.row)
                .map(|index| Command::DrillInto { slice_number: index + 1 }),
            MouseEventKind::Down(MouseButton::Right) => Some(Command::Up),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MouseEvent};

    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })
    }

    fn mouse(kind: MouseEventKind, column: u16, row: u16) -> Event {
        Event::Mouse(MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        })
    }

    fn targets_with_two_slices() -> ClickTargets {
        // Pie rows 0..2, blank row 2, legend rows 3 (slice 0) and 4 (slice 1).
        ClickTargets {
            cells: vec![
                vec![Some(0), Some(0), Some(1), Some(1)],
                vec![Some(0), Some(0), Some(1), Some(1)],
                vec![None,    None,    None,    None   ],
                vec![Some(0), Some(0), Some(0), Some(0)],
                vec![Some(1), Some(1), Some(1), Some(1)],
            ],
        }
    }

    #[test]
    fn pressing_q_quits() {
        let targets = ClickTargets::empty();
        assert_eq!(
            event_to_command(&key(KeyCode::Char('q')), &targets),
            Some(Command::Quit)
        );
        assert_eq!(
            event_to_command(&key(KeyCode::Char('Q')), &targets),
            Some(Command::Quit)
        );
    }

    #[test]
    fn pressing_escape_quits() {
        let targets = ClickTargets::empty();
        assert_eq!(
            event_to_command(&key(KeyCode::Esc), &targets),
            Some(Command::Quit)
        );
    }

    #[test]
    fn pressing_u_goes_up_as_a_keyboard_shortcut() {
        let targets = ClickTargets::empty();
        assert_eq!(
            event_to_command(&key(KeyCode::Char('u')), &targets),
            Some(Command::Up)
        );
    }

    #[test]
    fn pressing_d_opens_the_drive_picker() {
        let targets = ClickTargets::empty();
        assert_eq!(
            event_to_command(&key(KeyCode::Char('d')), &targets),
            Some(Command::ChangeDrive)
        );
        assert_eq!(
            event_to_command(&key(KeyCode::Char('D')), &targets),
            Some(Command::ChangeDrive)
        );
    }

    #[test]
    fn pressing_r_refreshes() {
        let targets = ClickTargets::empty();
        assert_eq!(
            event_to_command(&key(KeyCode::Char('r')), &targets),
            Some(Command::Refresh)
        );
    }

    #[test]
    fn unmapped_keys_produce_no_command() {
        let targets = ClickTargets::empty();
        assert_eq!(event_to_command(&key(KeyCode::Char('x')), &targets), None);
        assert_eq!(event_to_command(&key(KeyCode::Enter), &targets), None);
    }

    #[test]
    fn pressing_a_digit_one_through_nine_drills_into_that_slice() {
        let targets = ClickTargets::empty();
        for digit in 1..=9 {
            let ch = std::char::from_digit(digit as u32, 10).unwrap();
            assert_eq!(
                event_to_command(&key(KeyCode::Char(ch)), &targets),
                Some(Command::DrillInto { slice_number: digit }),
                "digit {digit} should drill into slice {digit}"
            );
        }
    }

    #[test]
    fn pressing_zero_does_nothing_because_legend_starts_at_one() {
        let targets = ClickTargets::empty();
        assert_eq!(event_to_command(&key(KeyCode::Char('0')), &targets), None);
    }

    #[test]
    fn left_click_on_a_slice_drills_into_it() {
        let targets = targets_with_two_slices();
        // (0, 0) is in the slice 0 region
        assert_eq!(
            event_to_command(
                &mouse(MouseEventKind::Down(MouseButton::Left), 0, 0),
                &targets
            ),
            Some(Command::DrillInto { slice_number: 1 })
        );
        // (3, 0) is in the slice 1 region
        assert_eq!(
            event_to_command(
                &mouse(MouseEventKind::Down(MouseButton::Left), 3, 0),
                &targets
            ),
            Some(Command::DrillInto { slice_number: 2 })
        );
    }

    #[test]
    fn left_click_on_a_legend_line_drills_into_that_slice() {
        let targets = targets_with_two_slices();
        // pie_height = 2, blank at row 2, legend rows 3 and 4
        assert_eq!(
            event_to_command(
                &mouse(MouseEventKind::Down(MouseButton::Left), 0, 3),
                &targets
            ),
            Some(Command::DrillInto { slice_number: 1 })
        );
        // Anywhere across the legend line's rendered span resolves to its slice.
        assert_eq!(
            event_to_command(
                &mouse(MouseEventKind::Down(MouseButton::Left), 3, 4),
                &targets
            ),
            Some(Command::DrillInto { slice_number: 2 })
        );
    }

    #[test]
    fn left_click_outside_any_slice_produces_no_command() {
        let targets = targets_with_two_slices();
        assert_eq!(
            event_to_command(
                &mouse(MouseEventKind::Down(MouseButton::Left), 99, 99),
                &targets
            ),
            None
        );
    }

    #[test]
    fn right_click_anywhere_goes_up() {
        let targets = targets_with_two_slices();
        assert_eq!(
            event_to_command(
                &mouse(MouseEventKind::Down(MouseButton::Right), 0, 0),
                &targets
            ),
            Some(Command::Up)
        );
        assert_eq!(
            event_to_command(
                &mouse(MouseEventKind::Down(MouseButton::Right), 99, 99),
                &targets
            ),
            Some(Command::Up)
        );
    }

    #[test]
    fn mouse_movement_without_a_button_press_is_ignored() {
        let targets = targets_with_two_slices();
        assert_eq!(
            event_to_command(&mouse(MouseEventKind::Moved, 0, 0), &targets),
            None
        );
    }

    #[test]
    fn terminal_resize_events_are_ignored() {
        let targets = ClickTargets::empty();
        assert_eq!(
            event_to_command(&Event::Resize(80, 24), &targets),
            None
        );
    }
}
