use std::fs;
use std::path::PathBuf;

use base64::Engine as _;
use base64::engine::general_purpose;
use chrono::{DateTime, Utc};
use filetime::FileTime;

pub async fn write_file(filename: &str, content: &str, mtime: &str) -> anyhow::Result<()> {
    let file_path = PathBuf::from(filename);

    // if the content is the same, return
    if file_path.exists() {
        let bytes = fs::read(&file_path)?;
        let encoded = general_purpose::STANDARD.encode(&bytes);
        if encoded == content {
            return Ok(());
        }
    }

    let decoded = general_purpose::STANDARD.decode(content)?;
    let mtime: DateTime<Utc> = DateTime::parse_from_rfc3339(mtime)?.with_timezone(&Utc);

    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(&file_path, decoded)?;

    let file_time = FileTime::from_unix_time(mtime.timestamp(), 0);
    filetime::set_file_mtime(&file_path, file_time)?;

    Ok(())
}
