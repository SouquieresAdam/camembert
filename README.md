# 🧀 Camembert

**Interactive disk-usage explorer for your terminal.** Scans a folder, renders the result as a Unicode pie chart with a colored legend, and lets you drill down with the mouse or keyboard.

Standalone binary — no runtime needed; install via `cargo install`.

```
📁 C:\Users\Adam

           █████████                  [1] █ Documents       42.0%   210 GB
        ████████████████              [2] ▓ Videos          28.0%   140 GB
      ███████████▓▓▓▓▓▓▓▓             [3] ▒ Photos          18.0%    90 GB
     ████████▓▓▓▓▓▓▓▓▓▓▓▓░░           [4] ░ Music            7.0%    35 GB
     ███▓▓▓▓▓▓▓▓▓▓▓▓▓▓░░░░░           [5] # node_modules     3.5%    18 GB
      ██▓▓▓▓▓▓▓▓▓▓░░░░░░░             [6] @ Autres           1.5%     7 GB
       ████▓▓▓▓░░░░░░░░
        █████░░░░░░░░
           ░░░░

1-9 = drill · U = up · D = change drive · R = refresh · Q = quit
```

## Features

- **Unicode pie chart** of folder sizes, with a numbered legend (`[1]`, `[2]`, …)
- **Responsive layout**: pie radius scales to fill the terminal; switches to a side-by-side legend when there's room
- **Color-coded slices** (10-color cycling palette)
- **Mouse and keyboard** navigation (left-click/digits to drill, right-click/`U` to go up)
- **Live progress** during scans:
  - spinner is sized like the eventual pie (no UI jump)
  - the new level's folder names appear immediately, with a blinking cursor where the size will land
  - each top-level subtree's size fills in the moment its walk completes
  - the footer shows the deepest path currently being walked
- **Drive picker** (`D` key) with per-drive occupation bars
- **Smart cache**: subtrees you've already walked are reused — going up to a parent re-walks only new siblings, not the one you came from

## Install

### Easy install (no terminal experience required)

#### Windows

1. Download [`camembert-x86_64-pc-windows-msvc.msi`](https://github.com/SouquieresAdam/camembert/releases/latest) from the latest release
2. Double-click the `.msi` file
3. If Windows shows **"Windows protected your PC"**, click **More info** → **Run anyway** (the installer is unsigned because we don't pay for an EV certificate — this is normal for new open-source tools)
4. Follow the wizard
5. Open **PowerShell** or **Terminal** and type `camembert`

#### macOS

```bash
brew install SouquieresAdam/camembert/camembert
```

(Requires [Homebrew](https://brew.sh). Homebrew handles macOS Gatekeeper for you — no scary "developer cannot be verified" dialog.)

#### Linux

For now, use the one-liner below. A `.deb` package is on the roadmap.

### Terminal one-liners (any OS)

**macOS / Linux**

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/SouquieresAdam/camembert/releases/latest/download/camembert-installer.sh | sh
```

**Windows (PowerShell)**

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/SouquieresAdam/camembert/releases/latest/download/camembert-installer.ps1 | iex"
```

These installers fetch the right binary for your OS/arch and drop it into `~/.cargo/bin` (or `%CARGO_HOME%\bin`). Pin a specific version by replacing `latest/download` with `download/v0.1.1`.

### From source (requires Rust 1.85+, edition 2024)

```bash
cargo install --git https://github.com/SouquieresAdam/camembert
```

Or from a local clone:

```bash
git clone https://github.com/SouquieresAdam/camembert
cd camembert
cargo install --path .
```

The `camembert` binary lands in `~/.cargo/bin`.

## Usage

```bash
camembert C:\path             # TUI mode (default)
camembert C:\path --no-mouse  # plain-text REPL fallback
camembert                     # scans the current directory
```

If stdin isn't a TTY, the program automatically falls back to REPL mode.

### Key bindings (TUI)

| Action | Mouse | Keyboard |
|---|---|---|
| Drill into a slice | left-click on the slice or its legend row | `1`–`9` |
| Go up one level | right-click | `U` |
| Refresh / re-scan | — | `R` |
| Change drive | — | `D` (also `U` at a disk root) |
| Quit | — | `Q`, `Esc` |

### Drive picker

Pressing `D` (or `U` at a disk root) opens a full-screen list of mounted drives with per-drive occupation:

```
💽 Choisissez un lecteur

  [1] C:\  ████████████████░░░░  78.5%   391 GB / 500 GB
  [2] D:\  ████░░░░░░░░░░░░░░░░  20.0%    80 GB / 400 GB

1-9 ou clic = entrer · Q ou Esc = annuler
```

## Architecture

Library + binary (both named `camembert`):

```
src/
├── lib.rs                  DiskEntry / EntryKind
├── main.rs                 binary entry point (CLI args, TUI vs REPL)
├── splash.txt              ASCII banner (compiled in via include_str!)
├── aggregator.rs           sort + "Autres" bucket
├── command.rs              REPL parser
├── drives.rs               drive enumeration + progress bar
├── drive_picker.rs         drive picker UI
├── event_map.rs            crossterm Event → Command (pure)
├── scanner.rs              recursive scan + cache + progress callback
├── tui.rs                  main TUI loop (scan thread, render, events)
└── render/                 pure rendering primitives
    ├── cheese_spinner.rs
    ├── layout.rs           responsive radius and stacked/side-by-side mode
    ├── mouse_target.rs     2D click-target grid
    ├── palette.rs          per-slice colors
    └── renderer.rs         pie + legend + scan/progress/skeleton views
```

Pure logic (parsing, layout, rendering, click mapping, progress merging) lives in modules with co-located unit tests and no terminal/filesystem dependencies. The TUI loop and the threaded scanner are the only places that hold IO.

## Development

```bash
cargo test                  # 146 tests, must always stay green
cargo run -- .              # run against the current directory
cargo build --release
```

The codebase is **TDD**: each pure module has a `#[cfg(test)]` suite co-located in the same file.

## Dependencies

- [`crossterm`](https://crates.io/crates/crossterm) — cross-platform terminal control (raw mode, mouse capture, colors)
- [`fs2`](https://crates.io/crates/fs2) — disk total/available space (drive picker)
- [`tempfile`](https://crates.io/crates/tempfile) — dev-only, for filesystem tests

## License

MIT — see [LICENSE](LICENSE). Free to use, modify, and redistribute, including in commercial products.
