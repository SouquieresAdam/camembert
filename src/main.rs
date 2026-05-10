use camembert::command::{Command, parse_command};
use camembert::render::renderer;
use camembert::{DiskEntry, aggregator, drives, scanner, tui};
use std::env;
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;

const DEFAULT_MAX_SLICES: usize = 8;
const FLAG_NO_MOUSE: &str = "--no-mouse";

fn main() -> ExitCode {
    let raw_args: Vec<String> = env::args().skip(1).collect();
    let no_mouse_flag = raw_args.iter().any(|arg| arg == FLAG_NO_MOUSE);
    let positional_path = raw_args
        .iter()
        .find(|arg| !arg.starts_with("--"))
        .map(PathBuf::from);

    let initial_folder = positional_path
        .unwrap_or_else(|| env::current_dir().expect("cannot read current directory"));

    let stdin_is_terminal = io::stdin().is_terminal();
    if no_mouse_flag || !stdin_is_terminal {
        run_repl(initial_folder)
    } else {
        match tui::run(initial_folder.clone()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("Mode souris indisponible ({err}) — bascule en mode REPL.");
                run_repl(initial_folder)
            }
        }
    }
}

// --- REPL fallback ---

fn run_repl(initial_folder: PathBuf) -> ExitCode {
    let mut current_folder = initial_folder;
    let stdin = io::stdin();
    let mut stdin_lines = stdin.lock().lines();

    loop {
        match run_one_view(&current_folder) {
            Ok(pie) => match read_command(&mut stdin_lines) {
                Some(cmd) => {
                    if !apply_command(cmd, &mut current_folder, &pie) {
                        return ExitCode::SUCCESS;
                    }
                }
                None => return ExitCode::SUCCESS,
            },
            Err(err) => {
                eprintln!("Erreur lors du scan de {} : {}", current_folder.display(), err);
                if !try_recover_by_going_up(&mut current_folder) {
                    return ExitCode::FAILURE;
                }
            }
        }
    }
}

fn run_one_view(folder: &PathBuf) -> io::Result<Vec<DiskEntry>> {
    println!("\n📁 {}", folder.display());
    let entries = scanner::scan_first_level(folder)?;
    let pie = aggregator::aggregate(entries, DEFAULT_MAX_SLICES);
    print!("{}", renderer::render(&pie));
    println!("\nCommandes : [N] drill-down · [u] remonter · [q] quitter");
    print!("> ");
    io::stdout().flush().ok();
    Ok(pie)
}

fn read_command(lines: &mut impl Iterator<Item = io::Result<String>>) -> Option<Command> {
    lines.next().and_then(|line| line.ok()).map(|s| parse_command(&s))
}

fn apply_command(cmd: Command, current: &mut PathBuf, pie: &[DiskEntry]) -> bool {
    match cmd {
        Command::Quit => false,
        Command::Refresh => true,
        Command::Up => {
            match current.parent() {
                Some(parent) => *current = parent.to_path_buf(),
                None => println!("(déjà à la racine, on ne peut pas remonter)"),
            }
            true
        }
        Command::DrillInto { slice_number } => {
            if slice_number == 0 || slice_number > pie.len() {
                println!(
                    "(numéro {} hors de portée — il y a {} tranches)",
                    slice_number,
                    pie.len()
                );
                return true;
            }
            let target = &pie[slice_number - 1];
            if !target.is_drillable() {
                println!(
                    "(la tranche [{}] « {} » n'est pas un dossier — drill-down impossible)",
                    slice_number, target.name
                );
                return true;
            }
            *current = current.join(&target.name);
            true
        }
        Command::ChangeDrive => {
            print_drives_in_repl();
            if let Some(picked) = read_drive_choice() {
                *current = picked;
            }
            true
        }
        Command::Unknown(input) => {
            println!("(commande inconnue : « {} »)", input);
            true
        }
    }
}

fn print_drives_in_repl() {
    let drives_list = drives::list_drives();
    if drives_list.is_empty() {
        println!("(aucun lecteur monté)");
        return;
    }
    println!("\n💽 Lecteurs disponibles :");
    for (i, drive) in drives_list.iter().enumerate() {
        let bar = drives::progress_bar(drive.ratio(), 20);
        println!(
            "  [{}] {}  {}  {:>5.1}%   {} / {}",
            i + 1,
            drive.path.display(),
            bar,
            drive.ratio() * 100.0,
            renderer::humanize_bytes(drive.used),
            renderer::humanize_bytes(drive.total),
        );
    }
    print!("Numéro du lecteur > ");
    io::stdout().flush().ok();
}

fn read_drive_choice() -> Option<PathBuf> {
    let mut line = String::new();
    if io::stdin().read_line(&mut line).is_err() {
        return None;
    }
    let trimmed = line.trim();
    let n: usize = trimmed.parse().ok()?;
    let drives_list = drives::list_drives();
    drives_list.get(n.checked_sub(1)?).map(|d| d.path.clone())
}

fn try_recover_by_going_up(current: &mut PathBuf) -> bool {
    match current.parent() {
        Some(parent) => {
            *current = parent.to_path_buf();
            true
        }
        None => false,
    }
}
