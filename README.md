# TuxPad++

A feature-rich text editor for Linux inspired by Notepad++, built with Rust and GTK4.

## Features

- **Multi-tab editing** – open multiple files in scrollable, reorderable tabs with individual close buttons
- **Syntax highlighting** – powered by GtkSourceView5 with automatic language detection from file extension
- **Line numbers** and current-line highlighting
- **Auto-indent**, smart backspace, and 4-space tab expansion
- **Regex search** – `Ctrl+F` toggles a search bar with live match highlighting and match count
- **Code formatting (Beautify)** – delegates to external formatters based on the detected language:

  | Language              | Tool          |
  |-----------------------|---------------|
  | Rust                  | `rustfmt`     |
  | JS / TS / CSS / HTML / JSON | `prettier` |
  | Python                | `black`       |
  | C / C++               | `clang-format`|

- **Diff view** – compare the on-disk version of a file with the current (unsaved) editor content; added lines are shown in green, removed lines in red
- **Status bar** – real-time cursor position (line / column)
- **File actions** – New Tab, Open File, Save, Save As via the hamburger menu

## Keyboard Shortcuts

| Shortcut  | Action          |
|-----------|-----------------|
| `Ctrl+T`  | New tab         |
| `Ctrl+F`  | Toggle search   |
| `Ctrl+S`  | Save file       |

## Requirements

| Dependency      | Minimum version |
|-----------------|-----------------|
| GTK             | 4.14            |
| Libadwaita      | 1.5             |
| GtkSourceView5  | 5.12            |
| Rust toolchain  | 2021 edition    |

On Fedora / RHEL:
```bash
sudo dnf install gtk4-devel libadwaita-devel gtksourceview5-devel
```

On Debian / Ubuntu:
```bash
sudo apt install libgtk-4-dev libadwaita-1-dev libgtksourceview-5-dev
```

## Build & Run

```bash
# Debug build
cargo run

# Release build
cargo build --release
./target/release/tuxpad
```

## Project Structure

```
src/
├── main.rs       – Application entry point, window layout, keyboard shortcuts, and actions
├── editor.rs     – EditorView: GtkSourceView5 widget backed by a ropey::Rope buffer
├── formatter.rs  – Formatter: spawns external formatter tools (rustfmt, prettier, black, …)
└── diff.rs       – DiffResult + GTK4 dialog for line-by-line diff display
```

## License

MIT
