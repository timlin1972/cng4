use std::env;
use std::path::Path;
use std::process::Command;

use regex::Regex;
use sysinfo::Networks;
use unicode_width::UnicodeWidthStr;
use walkdir::WalkDir;

use crate::consts;

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

pub fn level_str(level: &str) -> &str {
    match level.to_lowercase().as_str() {
        "info" => "I",
        "warn" => "W",
        "error" => "E",
        _ => "?",
    }
}

pub fn level_to_str(level: &log::Level) -> &str {
    match level {
        log::Level::Info => "I",
        log::Level::Warn => "W",
        log::Level::Error => "E",
        _ => "?",
    }
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

//
// Tailscale helpers
//

const TAILSCALE_INTERFACE: &str = "tailscale";
const TAILSCALE_INTERFACE_MAC: &str = "utun";

pub fn get_tailscale_ip() -> Option<String> {
    let networks = Networks::new_with_refreshed_list();
    for (interface_name, network) in &networks {
        if interface_name.starts_with(TAILSCALE_INTERFACE) {
            for ipnetwork in network.ip_networks().iter() {
                // if ipv4
                if ipnetwork.addr.is_ipv4() {
                    return Some(ipnetwork.addr.to_string());
                }
            }
        }
        if interface_name.starts_with(TAILSCALE_INTERFACE_MAC) {
            for ipnetwork in network.ip_networks().iter() {
                // if ipv4
                if let std::net::IpAddr::V4(ip) = ipnetwork.addr {
                    // if the first 1 byte is 100, it's a tailscale ip
                    if ip.octets()[0] == 100 {
                        return Some(ipnetwork.addr.to_string());
                    }
                }
            }
        }
    }

    let output = Command::new("ifconfig")
        .output()
        .expect("Failed to execute ifconfig");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // 找出 tun0 區塊
    let tun0_block = stdout
        .split("\n\n")
        .find(|block| block.contains("tun0"))
        .expect("tun0 interface not found");

    // 用 regex 抓出 inet IP
    let re = Regex::new(r"inet (\d+\.\d+\.\d+\.\d+)").unwrap();
    if let Some(caps) = re.captures(tun0_block) {
        let ip = &caps[1];
        return Some(ip.to_string());
    }

    None
}

pub fn get_tailscale_ip_str(tailscale_ip: &Option<String>) -> String {
    match tailscale_ip {
        Some(ip) => ip.clone(),
        None => consts::NA.to_string(),
    }
}

pub fn list_files(folder: &str) -> Vec<String> {
    let mut output = Vec::new();

    output.push("  Files:".to_string());
    let mut has_files = false;
    for entry in WalkDir::new(folder)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        if let Some(path) = entry.path().to_str() {
            // Get the relative path to the folder
            if let Some(rel_path) = path.strip_prefix(folder) {
                let rel_path = rel_path.trim_start_matches('/').to_string();
                has_files = true;
                output.push(format!("    - {}", rel_path));
            }
        }
    }
    if !has_files {
        output.push("    (no files)".to_string());
    }

    output
}

pub fn shorten(s: &str, prefix: usize, suffix: usize) -> String {
    let len = s.chars().count();

    if len <= prefix + suffix {
        s.to_string()
    } else {
        let prefix: String = s.chars().take(prefix).collect();
        let suffix: String = s
            .chars()
            .rev()
            .take(suffix)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        format!("{prefix}...{suffix}")
    }
}
