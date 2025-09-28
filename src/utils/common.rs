use std::env;
use std::path::Path;

use clap::ValueEnum;

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

    let action = match Action::from_str(action, false) {
        Ok(action) => action,
        Err(_) => return Err(format!("Unknown action: `{action}` for command: `{cmd}`")),
    };

    Ok((cmd_parts, action))
}
