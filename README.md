# cng4

Center Next Generation 4

# Features

- Usage

```
Center Generation 4th v0.1.0

Usage: cng4 [OPTIONS]

Options:
      --mode <cli|gui>     [default: gui] [possible values: cli, gui]
      --script <filename>  [default: cfg.toml]
  -h, --help               Print help
  -V, --version            Print version

Examples:
  cng4 --mode cli --script custom_config.toml
  cng4 --mode gui
  cng4                 # Runs in GUI mode with cfg.toml

Description:
  cng4 is a information tool that supports both CLI and GUI modes.
  Use --mode to select the interface, and --script to provide a custom configuration file.
```

# Code flow

```
* handle panic
* create channels
  - msg
  - shutdown
* startup messages
* hanele args
* plugins
* insert plugin log
  - new
* insert plugin cfg
  - new
  - init
    - name
    - running scripts
      - inser plugin panels
```

# Implementation

1. argument parsing
1. handle panic
1. messages
1. log
1. cfg
1. system
1. cli
1. web
1. music
1. panels
1. gui

# Configuration file

- Format: TOML
- Keys
  - name
  - script_gui
  - script_cli

# How to add a plugin

1. add plugin_xxx
1. modify plugins/mod.rs
1. modify plugins/plugins_main.rs (insert)
1. modify cfg.toml

# Web APIs

- /hello

```
curl http://localhost:9759
```

- /cmd

```
curl -X POST http://localhost:9759/cmd -H "Content-Type: application/json" -d '{"cmd": "p plugins show"}'
```

# Keyboard

- TAB
- Up
- Down
- Left
- Right
- Ctrl-c
- Ctrl-w
- Ctrl-s
- Ctrl-a
- Ctrl-d

# Test

- Web APIs
  - `test/web.sh`

# yt-dlp

## Example

```
yt-dlp --output "%(title)s.%(ext)s" --embed-thumbnail --add-metadata --extract-audio --audio-format mp3 --audio-quality 320K "https://www.youtube.com/watch?v=duZDsG3tvoA"
```

## Update

```
sudo wget https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp -O /usr/local/bin/yt-dlp
sudo chmod a+rx /usr/local/bin/yt-dlp
```
