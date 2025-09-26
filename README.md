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

# Implementation

1. argument parsing
2. handle panic
3. messages
4. log
5. cfg
6. system

# Configuration file

- Format: TOML
- Keys
  - name
  - plugins
  - script_gui
  - script_cli

# How to add a plugin

1. add plugin_xxx
2. modify plugins/mod.rs
3. modify plugins/plugins_main.rs (insert)
4. modify cfg.toml
