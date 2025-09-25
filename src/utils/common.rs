use clap::ValueEnum;

use crate::messages::Action;

pub fn get_cmd_action(cmd: &str) -> Result<(Vec<String>, Action), String> {
    let cmd_parts = match shell_words::split(cmd) {
        Ok(parts) => parts,
        Err(_) => return Err(format!("Unknown message format: `{cmd}`")),
    };

    let action = match cmd_parts.get(2) {
        Some(action) => action,
        None => return Err(format!("Incomplete command: `{cmd}`")),
    };

    let action = match Action::from_str(action, true) {
        Ok(action) => action,
        Err(_) => return Err(format!("Unknown action: `{action}` for command: `{cmd}`")),
    };

    Ok((cmd_parts, action))
}
