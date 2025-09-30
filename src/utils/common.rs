use std::env;
use std::path::Path;

use unicode_width::UnicodeWidthStr;

use crate::consts;
use crate::messages::Action;

pub fn get_binary_name() -> String {
    if let Ok(path) = env::current_exe()
        && let Some(name) = path.file_name()
    {
        return name.to_string_lossy().into_owned();
    }

    panic!(
        "Failed to get binary name from path: {:?}",
        Path::new(&env::args().next().unwrap())
    );
}

pub fn get_cmd_action(cmd: &str) -> Result<(Vec<String>, Action), String> {
    let cmd_parts = match shell_words::split(cmd) {
        Ok(parts) => parts,
        Err(_) => return Err(format!("Unknown message format: `{cmd}`")),
    };

    let action = match cmd_parts.get(2) {
        Some(action) => action,
        None => return Err(format!("Incomplete command: `{cmd}`")),
    };

    let action: Action = match action.parse() {
        Ok(action) => action,
        Err(_) => return Err(format!("Unknown action: `{action}` for command: `{cmd}`")),
    };

    Ok((cmd_parts, action))
}

pub fn temperature_str(temperature: Option<f32>) -> String {
    match temperature {
        Some(t) => format!("{:.1}°C", t),
        None => consts::NA.to_string(),
    }
}

pub fn pad_str(s: &str, total_width: usize) -> String {
    let display_width = UnicodeWidthStr::width(s);
    let padding = total_width.saturating_sub(display_width);
    format!("{}{}", s, " ".repeat(padding))
}

pub enum MsgTemplate {
    UnsupportedAction,
    MissingParameters,
    InvalidParameters,
}

impl MsgTemplate {
    pub fn format(&self, arg1: &str, arg2: &str, arg3: &str) -> String {
        match self {
            MsgTemplate::UnsupportedAction => format!("Unsupported action: `{arg1}`"),
            MsgTemplate::MissingParameters => {
                format!("Missing {arg1} for `{arg2}` command: `{arg3}`")
            }
            MsgTemplate::InvalidParameters => {
                format!("Invalid {arg1} for `{arg2}` command: `{arg3}`")
            }
        }
    }
}
