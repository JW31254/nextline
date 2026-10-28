//! Update check: asks GitHub for the latest release and compares it with the running version.
//!
//! Notify-only. Nothing is downloaded or executed; the user is pointed at the release page.
//! The request goes through WinHTTP so it honours the system proxy and certificate store.

use serde::Deserialize;

pub const REPO: &str = "JW31254/nextline";
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

const MAX_RESPONSE_BYTES: usize = 512 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateInfo {
    /// Bare version such as "0.3.0" (no leading "v").
    pub version: String,
    /// Release page on GitHub; always under `https://github.com/<REPO>/`.
    pub url: String,
    /// First lines of the release notes, for a tooltip.
    pub summary: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum CheckError {
    /// Offline, blocked or timed out.
    Network,
    /// GitHub answered, but not with a usable release (rate limited, no releases yet...).
    Unavailable(u32),
    /// The response could not be understood.
    BadResponse,
}

impl std::fmt::Display for CheckError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CheckError::Network => write!(f, "Couldn't reach GitHub."),
            CheckError::Unavailable(404) => write!(f, "No releases are published yet."),
            CheckError::Unavailable(code) => write!(f, "GitHub answered with status {code}."),
            CheckError::BadResponse => write!(f, "Unexpected response from GitHub."),
        }
    }
}

/// Parses "v1.2.3", "1.2" or "1.2.3-beta.1" into comparable numbers. Pre-release tails are ignored.
pub fn parse_version(text: &str) -> Option<(u32, u32, u32)> {
    let core = text.trim().trim_start_matches(['v', 'V']);
    let core = core.split(['-', '+']).next()?;
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().map(str::parse).transpose().ok()?.unwrap_or(0);
    let patch = parts.next().map(str::parse).transpose().ok()?.unwrap_or(0);
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse_version(candidate), parse_version(current)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

/// Only release pages of this repository may be opened from the banner.
pub fn is_trusted_release_url(url: &str) -> bool {
    let prefix = format!("https://github.com/{REPO}/");
    url.starts_with(&prefix) && !url.contains(['\\', ' ', '\n', '\r', '@'])
}

#[derive(Deserialize)]
struct ReleaseJson {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

/// Turns the body of `GET /repos/<repo>/releases/latest` into an update, if it is newer than `current`.
pub fn parse_release(json: &str, current: &str) -> Result<Option<UpdateInfo>, CheckError> {
    let release: ReleaseJson = serde_json::from_str(json).map_err(|_| CheckError::BadResponse)?;
    if release.draft || release.prerelease {
        return Ok(None);
    }
    let version = release.tag_name.trim().trim_start_matches(['v', 'V']).to_string();
    parse_version(&version).ok_or(CheckError::BadResponse)?;
    if !is_newer(&version, current) {
        return Ok(None);
    }
    if !is_trusted_release_url(&release.html_url) {
        return Err(CheckError::BadResponse);
    }
    let summary = release
        .body
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("SHA-256"))
        .take(6)
        .collect::<Vec<_>>()
        .join("\n");
    Ok(Some(UpdateInfo { version, url: release.html_url, summary }))
}

/// Fetches the latest release and reports it if newer than the running version.
pub fn check_latest() -> Result<Option<UpdateInfo>, CheckError> {
    let body = http::get_https("api.github.com", &format!("/repos/{REPO}/releases/latest"))?;
    parse_release(&body, CURRENT_VERSION)
}

pub fn now_unix() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Hides an update the user chose to skip.
pub fn unless_skipped(info: Option<UpdateInfo>, skipped_version: &str) -> Option<UpdateInfo> {
    info.filter(|i| i.version != skipped_version)
}

mod http {
    use super::{CURRENT_VERSION, CheckError, MAX_RESPONSE_BYTES};
    use std::ffi::c_void;
    use windows_sys::Win32::Networking::WinHttp::*;

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Closes a WinHTTP handle when dropped.
    struct Handle(*mut c_void);
    impl Handle {
        fn new(raw: *mut c_void) -> Result<Handle, CheckError> {
            if raw.is_null() { Err(CheckError::Network) } else { Ok(Handle(raw)) }
        }
    }
    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe { WinHttpCloseHandle(self.0) };
        }
    }

    pub fn get_https(host: &str, path: &str) -> Result<String, CheckError> {
        unsafe {
            let agent = wide(&format!("NextLine/{CURRENT_VERSION}"));
            let session = Handle::new(WinHttpOpen(
                agent.as_ptr(),
                WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
                std::ptr::null(),
                std::ptr::null(),
                0,
            ))?;
            // resolve, connect, send, receive (milliseconds)
            WinHttpSetTimeouts(session.0, 5_000, 5_000, 8_000, 8_000);

            let host_w = wide(host);
            let connection = Handle::new(WinHttpConnect(session.0, host_w.as_ptr(), INTERNET_DEFAULT_HTTPS_PORT, 0))?;

            let verb = wide("GET");
            let path_w = wide(path);
            let request = Handle::new(WinHttpOpenRequest(
                connection.0,
                verb.as_ptr(),
                path_w.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                WINHTTP_FLAG_SECURE,
            ))?;

            let headers = wide("Accept: application/vnd.github+json\r\nX-GitHub-Api-Version: 2022-11-28\r\n");
            if WinHttpSendRequest(request.0, headers.as_ptr(), u32::MAX, std::ptr::null(), 0, 0, 0) == 0 {
                return Err(CheckError::Network);
            }
            if WinHttpReceiveResponse(request.0, std::ptr::null_mut()) == 0 {
                return Err(CheckError::Network);
            }

            let mut status: u32 = 0;
            let mut size = std::mem::size_of::<u32>() as u32;
            if WinHttpQueryHeaders(
                request.0,
                WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
                std::ptr::null(),
                &mut status as *mut u32 as *mut c_void,
                &mut size,
                std::ptr::null_mut(),
            ) == 0
            {
                return Err(CheckError::Network);
            }
            if status != 200 {
                return Err(CheckError::Unavailable(status));
            }

            let mut body: Vec<u8> = Vec::new();
            let mut chunk = [0u8; 8192];
            loop {
                let mut read: u32 = 0;
                if WinHttpReadData(request.0, chunk.as_mut_ptr() as *mut c_void, chunk.len() as u32, &mut read) == 0 {
                    return Err(CheckError::Network);
                }
                if read == 0 {
                    break;
                }
                body.extend_from_slice(&chunk[..read as usize]);
                if body.len() > MAX_RESPONSE_BYTES {
                    return Err(CheckError::BadResponse);
                }
            }
            String::from_utf8(body).map_err(|_| CheckError::BadResponse)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str, url: &str, extra: &str) -> String {
        format!(r#"{{"tag_name":"{tag}","html_url":"{url}","body":"First line\n\nSecond line\nSHA-256: abc"{extra}}}"#)
    }

    /// Hits the real GitHub API through WinHTTP. Run with `cargo test -- --ignored` once a release exists.
    #[test]
    #[ignore = "needs network access and a published release"]
    fn live_lookup_reports_a_newer_release_than_0_0_1() {
        let body = http::get_https("api.github.com", &format!("/repos/{REPO}/releases/latest")).expect("reach GitHub");
        let info = parse_release(&body, "0.0.1").expect("parse").expect("a release newer than 0.0.1");
        assert!(is_trusted_release_url(&info.url));
        assert!(parse_release(&body, "999.0.0").unwrap().is_none());
    }

    #[test]
    fn skipped_versions_are_hidden() {
        let info = UpdateInfo { version: "0.3.0".into(), url: String::new(), summary: String::new() };
        assert!(unless_skipped(Some(info.clone()), "").is_some());
        assert!(unless_skipped(Some(info.clone()), "0.2.0").is_some());
        assert!(unless_skipped(Some(info), "0.3.0").is_none());
        assert!(unless_skipped(None, "").is_none());
    }

    #[test]
    fn parses_versions() {
        assert_eq!(parse_version("v0.2.0"), Some((0, 2, 0)));
        assert_eq!(parse_version("1.10"), Some((1, 10, 0)));
        assert_eq!(parse_version("2"), Some((2, 0, 0)));
        assert_eq!(parse_version("1.2.3-beta.1"), Some((1, 2, 3)));
        assert_eq!(parse_version("1.2.3.4"), None);
        assert_eq!(parse_version("latest"), None);
        assert_eq!(parse_version(""), None);
    }

    #[test]
    fn compares_numerically_not_textually() {
        assert!(is_newer("0.10.0", "0.9.0"));
        assert!(is_newer("v1.0.0", "0.99.99"));
        assert!(!is_newer("0.2.0", "0.2.0"));
        assert!(!is_newer("0.1.9", "0.2.0"));
        assert!(!is_newer("garbage", "0.2.0"));
    }

    #[test]
    fn only_trusts_this_repo() {
        assert!(is_trusted_release_url("https://github.com/JW31254/nextline/releases/tag/v0.3.0"));
        assert!(!is_trusted_release_url("https://github.com/JW31254/nextline-evil/releases/tag/v1"));
        assert!(!is_trusted_release_url("https://github.com.evil.example/JW31254/nextline/"));
        assert!(!is_trusted_release_url("http://github.com/JW31254/nextline/releases"));
        assert!(!is_trusted_release_url("https://github.com/JW31254/nextline/@evil"));
        assert!(!is_trusted_release_url("file:///C:/Windows/System32/calc.exe"));
    }

    #[test]
    fn reports_a_newer_release() {
        let json = release("v0.3.0", "https://github.com/JW31254/nextline/releases/tag/v0.3.0", "");
        let info = parse_release(&json, "0.2.0").unwrap().unwrap();
        assert_eq!(info.version, "0.3.0");
        assert_eq!(info.summary, "First line\nSecond line");
        assert!(info.url.ends_with("v0.3.0"));
    }

    #[test]
    fn ignores_same_older_draft_and_prerelease() {
        let url = "https://github.com/JW31254/nextline/releases/tag/v0.2.0";
        assert_eq!(parse_release(&release("v0.2.0", url, ""), "0.2.0").unwrap(), None);
        assert_eq!(parse_release(&release("v0.1.0", url, ""), "0.2.0").unwrap(), None);
        assert_eq!(parse_release(&release("v0.9.0", url, r#","draft":true"#), "0.2.0").unwrap(), None);
        assert_eq!(parse_release(&release("v0.9.0", url, r#","prerelease":true"#), "0.2.0").unwrap(), None);
    }

    #[test]
    fn rejects_foreign_urls_and_garbage() {
        let bad_url = release("v9.0.0", "https://example.org/download", "");
        assert_eq!(parse_release(&bad_url, "0.2.0"), Err(CheckError::BadResponse));
        assert_eq!(parse_release("not json", "0.2.0"), Err(CheckError::BadResponse));
        let bad_tag = release("nightly", "https://github.com/JW31254/nextline/releases/tag/nightly", "");
        assert_eq!(parse_release(&bad_tag, "0.2.0"), Err(CheckError::BadResponse));
    }
}
