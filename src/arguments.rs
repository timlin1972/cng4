use std::fmt;

use clap::{Parser, ValueEnum};

const DEFAULT_MODE: &str = "gui";
const DEFAULT_SCRIPT: &str = "cfg.toml";

#[derive(Parser, Debug)]
#[command(
    name = "cng4",
    version = env!("CARGO_PKG_VERSION"),
    about = concat!("Center Generation 4th v", env!("CARGO_PKG_VERSION")),
    after_help = "\
Examples:
  cng4 --mode cli --script custom_config.toml
  cng4 --mode gui
  cng4                 # Runs in GUI mode with cfg.toml

Description:
  cng4 is a information tool that supports both CLI and GUI modes.
  Use --mode to select the interface, and --script to provide a custom configuration file."
)]
pub struct Arguments {
    #[arg(long, value_enum, default_value = DEFAULT_MODE, value_name = "cli|gui")]
    pub mode: Mode,

    #[arg(long, default_value = DEFAULT_SCRIPT, value_name = "filename")]
    pub script: String,
}

#[derive(ValueEnum, Clone, Debug)]
pub enum Mode {
    Cli,
    Gui,
}

impl fmt::Display for Arguments {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "mode: {:?}, script: {}", self.mode, self.script)
    }
}
