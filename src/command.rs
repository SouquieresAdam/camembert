/// User actions accepted by the interactive REPL.
#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    /// Drill into the slice numbered `slice_number` (1-indexed, as shown in legend).
    DrillInto { slice_number: usize },
    /// Go back up one folder level.
    Up,
    /// Re-scan and re-render the current folder (e.g. on empty input).
    Refresh,
    /// Open the drive picker (Windows: A: through Z:; Unix: just `/`).
    /// Triggered by the `D` key, by typing `drive`/`drives`, or by `Up` at a
    /// disk root.
    ChangeDrive,
    /// Exit the program.
    Quit,
    /// Unrecognised input; carries the original text so the UI can echo it.
    Unknown(String),
}

pub fn parse_command(input: &str) -> Command {
    // Strip a leading BOM (PowerShell pipes inject one) before trimming.
    let without_bom = input.strip_prefix('\u{FEFF}').unwrap_or(input);
    let trimmed = without_bom.trim();
    if trimmed.is_empty() {
        return Command::Refresh;
    }

    let lowered = trimmed.to_ascii_lowercase();
    match lowered.as_str() {
        "u" | "up" | ".." => return Command::Up,
        "q" | "quit" | "exit" => return Command::Quit,
        "d" | "drive" | "drives" => return Command::ChangeDrive,
        _ => {}
    }

    if let Ok(slice_number) = trimmed.parse::<usize>() {
        if slice_number >= 1 {
            return Command::DrillInto { slice_number };
        }
    }

    Command::Unknown(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_a_slice_number_drills_into_that_slice() {
        assert_eq!(
            parse_command("1"),
            Command::DrillInto { slice_number: 1 }
        );
        assert_eq!(
            parse_command("7"),
            Command::DrillInto { slice_number: 7 }
        );
    }

    #[test]
    fn slice_number_input_tolerates_surrounding_whitespace() {
        assert_eq!(
            parse_command("  3  "),
            Command::DrillInto { slice_number: 3 }
        );
    }

    #[test]
    fn slice_number_zero_is_unknown_because_legend_is_one_indexed() {
        assert_eq!(parse_command("0"), Command::Unknown("0".to_string()));
    }

    #[test]
    fn typing_u_or_up_or_double_dot_goes_up_one_level() {
        assert_eq!(parse_command("u"), Command::Up);
        assert_eq!(parse_command("up"), Command::Up);
        assert_eq!(parse_command(".."), Command::Up);
    }

    #[test]
    fn up_command_is_case_insensitive() {
        assert_eq!(parse_command("U"), Command::Up);
        assert_eq!(parse_command("Up"), Command::Up);
        assert_eq!(parse_command("UP"), Command::Up);
    }

    #[test]
    fn typing_d_or_drive_or_drives_opens_the_drive_picker() {
        assert_eq!(parse_command("d"), Command::ChangeDrive);
        assert_eq!(parse_command("D"), Command::ChangeDrive);
        assert_eq!(parse_command("drive"), Command::ChangeDrive);
        assert_eq!(parse_command("drives"), Command::ChangeDrive);
    }

    #[test]
    fn typing_q_or_quit_or_exit_quits() {
        assert_eq!(parse_command("q"), Command::Quit);
        assert_eq!(parse_command("quit"), Command::Quit);
        assert_eq!(parse_command("exit"), Command::Quit);
    }

    #[test]
    fn empty_input_refreshes_the_view() {
        assert_eq!(parse_command(""), Command::Refresh);
        assert_eq!(parse_command("   "), Command::Refresh);
        assert_eq!(parse_command("\n"), Command::Refresh);
    }

    #[test]
    fn a_leading_byte_order_mark_does_not_break_parsing() {
        // PowerShell pipes prepend U+FEFF on the first line in some configurations.
        assert_eq!(
            parse_command("\u{FEFF}3"),
            Command::DrillInto { slice_number: 3 }
        );
        assert_eq!(parse_command("\u{FEFF}q"), Command::Quit);
    }

    #[test]
    fn unrecognised_input_is_returned_verbatim_for_echoing() {
        assert_eq!(
            parse_command("hello"),
            Command::Unknown("hello".to_string())
        );
        assert_eq!(
            parse_command("12abc"),
            Command::Unknown("12abc".to_string())
        );
    }
}
