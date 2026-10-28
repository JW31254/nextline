//! Crash-safe persistence under `%APPDATA%\NextLine`.

use crate::model::AppData;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

const APP_DIR: &str = "NextLine";

pub fn data_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join(APP_DIR)
}

fn library_path() -> PathBuf {
    data_dir().join("library.json")
}

pub fn load() -> AppData {
    let path = library_path();
    let parsed = fs::read_to_string(&path).ok().and_then(|text| serde_json::from_str::<AppData>(&text).ok());
    let mut data = match parsed {
        Some(data) => data,
        None => {
            // A corrupt file is kept aside rather than overwritten by the next save.
            if path.exists() {
                let _ = fs::rename(&path, path.with_extension("corrupt.json"));
            }
            AppData::default()
        }
    };
    data.sanitize();
    data
}

/// Writes to a temp file, flushes to disk, then swaps it in so a crash never truncates the library.
pub fn save(data: &AppData) -> std::io::Result<()> {
    let dir = data_dir();
    fs::create_dir_all(&dir)?;
    let path = library_path();
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(data).map_err(std::io::Error::other)?;
    {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(json.as_bytes())?;
        file.sync_all()?;
    }
    if path.exists() {
        let _ = fs::copy(&path, path.with_extension("bak"));
    }
    fs::rename(&tmp, &path)
}
