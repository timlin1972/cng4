use std::fmt;
use std::fs;
use std::path::PathBuf;

use base64::Engine as _;
use base64::engine::general_purpose;
use chrono::{DateTime, Utc};
use filetime::FileTime;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

pub fn encode(input: &[u8]) -> String {
    general_purpose::STANDARD.encode(input)
}

pub fn decode(input: &str) -> anyhow::Result<Vec<u8>> {
    let decoded = general_purpose::STANDARD.decode(input)?;
    Ok(decoded)
}

pub fn mtime_str_to_file_time(mtime: &str) -> anyhow::Result<FileTime> {
    let mtime: DateTime<Utc> = DateTime::parse_from_rfc3339(mtime)?.with_timezone(&Utc);
    Ok(FileTime::from_unix_time(mtime.timestamp(), 0))
}

pub fn mtime_str(metadata_modified: std::time::SystemTime) -> String {
    DateTime::<Utc>::from(metadata_modified).to_rfc3339()
}

fn hash_str(input: &str) -> String {
    let digest = Sha256::digest(input.as_bytes());
    hex::encode(digest)
}

pub async fn write_file(filename: &str, content: &str, mtime: &str) -> anyhow::Result<()> {
    let file_path = PathBuf::from(filename);

    // if the content is the same, return
    if file_path.exists() {
        let bytes = fs::read(&file_path)?;
        let encoded = encode(&bytes);
        if encoded == content {
            return Ok(());
        }
    }

    let decoded = decode(content)?;
    let file_time = mtime_str_to_file_time(mtime)?;

    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(&file_path, decoded)?;
    filetime::set_file_mtime(&file_path, file_time)?;

    Ok(())
}

#[derive(Deserialize, Serialize)]
pub struct FileMeta {
    pub filename: String,
    pub hash: String,
    pub mtime: String,
}

impl fmt::Display for FileMeta {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "file: {}, hash: {}, mtime: {}",
            self.filename, self.hash, self.mtime
        )
    }
}

#[derive(Deserialize, Serialize)]
pub struct FolderMeta {
    pub foldername: String,
    pub files: Vec<FileMeta>,
    pub hash: String,
}

impl fmt::Display for FolderMeta {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "\nfolder: {}", self.foldername)?;
        write!(f, "\nhash: {}", self.hash)?;
        for file in &self.files {
            write!(f, "\n  {}", file)?;
        }

        Ok(())
    }
}

pub fn get_folder_meta(foldername: &str) -> FolderMeta {
    let mut files = Vec::new();

    for entry in WalkDir::new(foldername)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        #[allow(clippy::collapsible_if)]
        if let Some(path) = entry.path().to_str() {
            if let Some(rel_path) = path.strip_prefix(foldername) {
                let rel_path = rel_path.trim_start_matches('/').to_string();
                if let Ok(metadata) = fs::metadata(entry.path()) {
                    if let Ok(modified) = metadata.modified() {
                        let mtime = mtime_str(modified);
                        if let Ok(bytes) = fs::read(entry.path()) {
                            let hash = hash_str(&encode(&bytes));
                            files.push(FileMeta {
                                filename: rel_path,
                                hash,
                                mtime,
                            });
                        }
                    }
                }
            }
        }
    }

    // Sort files by filename to ensure consistent hash
    files.sort_by(|a, b| a.filename.cmp(&b.filename));

    // Create a combined hash of all file hashes
    let combined_hash_input = files
        .iter()
        .map(|f| format!("{}:{}:{}", f.filename, f.hash, f.mtime))
        .collect::<Vec<_>>()
        .join("|");
    let folder_hash = hash_str(&combined_hash_input);

    FolderMeta {
        foldername: foldername.to_string(),
        files,
        hash: folder_hash,
    }
}
