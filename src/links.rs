//! Links shown in the About section of Settings.

pub const CONTACT_URL: &str = "https://www.linkedin.com/in/jack-wicks/";
pub const ISSUES_URL: &str = "https://github.com/JW31254/nextline/issues";
pub const SOURCE_URL: &str = "https://github.com/JW31254/nextline";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_are_https_and_clean() {
        for url in [CONTACT_URL, ISSUES_URL, SOURCE_URL] {
            assert!(url.starts_with("https://"), "{url}");
            assert!(!url.contains([' ', '\\', '@']), "{url}");
        }
    }
}
