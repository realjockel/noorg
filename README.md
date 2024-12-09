# Noorg

Capture everything, organize nothing.

Noorg is not just another note-taking tool—it's an editor-agnostic platform designed to integrate seamlessly with any text editor you prefer. Whether you're a fan of Vim, Emacs, VSCode, Obsidian or any other editor, Noorg empowers you to focus on what truly matters: capturing your thoughts and ideas without the burden of organization.

## A New Paradigm in Note Management

For those of us who struggle with organization, and dedicate too much time into creating the perfect system and note structure, **Noorg** offers a liberating approach. It shifts the focus from organizing to capturing thoughts and ideas effortlessly. By leveraging the power of Markdown and its frontmatter capabilities, Noorg allows you to annotate your notes with metadata, making them easily searchable and sortable without the need for manual organization.

But that's not all, Noorg is also a runtime that allows you to extend its functionality using Python, Lua, and Rust. This flexibility means you can tailor Noorg to fit unlimited use cases, from simple note-taking to complex data processing tasks.

## Extensible Runtime

At its core, Noorg is a highly extensible runtime, allowing you to enhance its capabilities using `Python` , `Lua` , and `Rust`. This flexibility means you can tailor Noorg to fit unlimited use cases, from simple note-taking to complex data processing tasks.

### Observer Pattern

Noorg employs the observer pattern to provide dynamic, real-time processing of your notes. Current observers, such as the time tracker and inline tags, are just examples of what's possible. These observers automatically process your notes, adding context and metadata without interrupting your flow. Imagine a daily journal that compiles all notes created on a specific day, or a system that tags notes based on content—these are just a few possibilities.

## Use Cases

- **Journal Creation**: Automatically compile a daily journal from notes created throughout the day.
- **Time Tracking**: Integrate time tracking to monitor how much time you spend on different topics.
- **Tagging System**: Use inline tags to categorize notes on-the-fly and to create dynamic views.
- **Dynamic Views**: Create dynamic views utilizing SQL to filter, sort and display your notes. (Comparable to Obsidian's Dataview plugin)
- **Kanban Board**: Create a kanban board to visualize your notes and tasks.
- **Custom Processing**: Use Python, Lua or Rust to process your notes and add custom metadata.
- **Lua executor**: Execute Lua inside your notes.
- **Unlimited Possibilities**: The possibilities are endless. You could build a system to automatically transcribe your notes, built presentations, call external APIs, integrate LLMs, and more.



## Caution: Pre-Alpha Software

Noorg is currently in a pre-alpha stage. While it offers powerful features, it is still under active development and may not be stable. We strongly advise starting slowly and backing up your note directory regularly. Experiment with Noorg in a safe environment to discover its potential without risking your important data.

## Join the Community

Noorg is for those minds, who want to break free from the constraints of traditional note-taking systems. It's editor agnostic, offline first, free, open source, community driven, and extensible. It's for thos who want to capture their thoughts, and not built the perfect organization system. Join the community in redefining how we manage our knowledge. Start using, contributing and building.


## Features
- Editor agnostic
- Runs as a system tray application which "watches" your note directory and automatically processes your notes
- Offline first, no cloud dependencies
- Extensible with Python, Lua and Rust
- SQL based dynamic views
- Kanban board
- Time tracker
- Inline tag detection and creation of Tag index
- Lua executor to execute Lua inside your notes

## Installation

### Prerequisites

1. Install Rust and Cargo:
```bash
# macOS/Linux
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Windows
# Download and run rustup-init.exe from https://rustup.rs
```

2. Install Python dependencies:
```bash
brew install python@3.9

# Add to ~/.zshrc or ~/.bashrc
export PYTHON_CONFIGURE_OPTS="--enable-framework"
export PYO3_PYTHON="/opt/homebrew/opt/python@3.9/bin/python3.9"
```
3. Install Lua

```bash
brew install lua # macOS
apt install lua5.4 # Debian, Ubuntu

# find lua path
lua -e "print(package.path:match('([^;]+)/?.lua'))"

# download json.lua dependency
curl -O https://raw.githubusercontent.com/rxi/json.lua/master/json.lua

# macOS: Copy to Lua package path
sudo cp json.lua /opt/homebrew/share/lua/5.4/json.lua

# Linux: Copy to Lua package path (typically one of these)
sudo cp json.lua /usr/local/share/lua/5.4/json.lua
# or
sudo cp json.lua /usr/share/lua/5.4/json.lua

# Verify installation
lua -e "require('json')"
```

### Option Build from Source

```bash
# Clone repository
git clone https://github.com/realjockel/noorg.git
cd noorg

# Build and install
cargo install --path .

# Build release binaries
cargo build --release
```


### Install Script

```bash
./install.sh
```
Uninstall with:
```bash
./install.sh uninstall
```

## Usage

### CLI Commands

Run `noorg` system tray application.
```bash
noorg
```

Run `noorg note_cli` to use the command line interface.
```bash
noorg note_cli
```

Run `noorg watch` to watch your note directory and automatically process your notes.
```bash
noorg note_cli watch
```

Run `noorg note_cli sync` to run all observers on all notes in your note directory.
```bash
noorg note_cli sync
```

Add a note:
```bash
noorg note_cli add --title "My Note" --body "Content" --frontmatter "tags:rust"

# Or without body (will open editor defined as EDITOR env variable to edit note)
noorg note_cli add --title "My Note" --frontmatter "tags:rust"

# Or with multiple frontmatter fields
noorg note_cli add -t "Rust Notes" -b "Discussed lifetimes" -f "priority:high" -f "project:X"
```


### System Tray Application

The system tray application provides quick access to:
- Note creation
- Settings
- Starting a watch to automatically process your notes on change

## Configuration
The configuration file (`config.toml`) is automatically created in the following locations depending on your operating system:

### Config Location
- **Linux**: `~/.config/norg/config.toml`
- **macOS**: `~/Library/Application Support/norg/config.toml`
- **Windows**: `C:\Users\<Username>\AppData\Roaming\norg\config.toml`

### Data Directory
Application data is stored in:
- **Linux**: `~/.local/share/norg/`
- **macOS**: `~/Library/Application Support/norg/`
- **Windows**: `C:\Users\<Username>\AppData\Local\norg\`

## Development Roadmap

- [ ] Fix query and list cli commands
- [ ] Add more tests
- [ ] Add more examples

## License

This project is licensed under Apache 2.0. See the [LICENSE](LICENSE) file for details.


