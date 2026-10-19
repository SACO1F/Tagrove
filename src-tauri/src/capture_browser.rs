//! Browser identities and registration rules shared by the desktop and core harness.
use crate::model::*;
use serde_json::Value;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Browser {
    Chrome,
    Edge,
}
impl Browser {
    pub const ALL: [Self; 2] = [Self::Chrome, Self::Edge];
    pub fn parse(value: &Value) -> Result<Self> {
        match value {
            Value::Null => Ok(Self::Chrome),
            Value::String(s) if s == "chrome" => Ok(Self::Chrome),
            Value::String(s) if s == "edge" => Ok(Self::Edge),
            _ => Err(err("INVALID_INPUT", "浏览器仅支持 Chrome 或 Edge")),
        }
    }
    pub fn id(self) -> &'static str {
        match self {
            Self::Chrome => "chrome",
            Self::Edge => "edge",
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Chrome => "Chrome",
            Self::Edge => "Edge",
        }
    }
    pub fn registry_key(self) -> &'static str {
        match self {
            Self::Chrome => r"Software\Google\Chrome\NativeMessagingHosts\local.shiqian.capture",
            Self::Edge => r"Software\Microsoft\Edge\NativeMessagingHosts\local.shiqian.capture",
        }
    }
}
pub fn registration_state(
    registration: Option<&Path>,
    expected: &Path,
    files_present: bool,
    files_current: bool,
) -> &'static str {
    match registration {
        Some(path) if path != expected => "foreign",
        None => "missing",
        _ if !files_present => "missing",
        _ if !files_current => "stale",
        _ => "ready",
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn legacy_client_defaults_to_chrome() {
        assert_eq!(Browser::parse(&Value::Null).unwrap(), Browser::Chrome);
    }
    #[test]
    fn browser_names_are_strict() {
        assert_eq!(Browser::parse(&json!("edge")).unwrap(), Browser::Edge);
        for value in [
            json!("Edge"),
            json!("firefox"),
            json!("../edge"),
            json!(true),
            json!(42),
            json!({}),
        ] {
            assert!(Browser::parse(&value).is_err());
        }
    }
    #[test]
    fn registration_keys_are_distinct_and_scoped() {
        assert_ne!(Browser::Chrome.registry_key(), Browser::Edge.registry_key());
        assert!(Browser::Edge
            .registry_key()
            .starts_with(r"Software\Microsoft\Edge\NativeMessagingHosts\"));
        assert!(Browser::Chrome
            .registry_key()
            .starts_with(r"Software\Google\Chrome\NativeMessagingHosts\"));
    }
    #[test]
    fn foreign_registration_is_never_reported_ready() {
        assert_eq!(
            registration_state(
                Some(Path::new("foreign.json")),
                Path::new("ours.json"),
                true,
                true
            ),
            "foreign"
        );
    }
    #[test]
    fn each_browser_can_be_ready_independently() {
        let manifest = Path::new("ours.json");
        assert_eq!(
            registration_state(Some(manifest), manifest, true, true),
            "ready"
        );
        assert_eq!(registration_state(None, manifest, true, true), "missing");
    }
    #[test]
    fn missing_or_changed_files_require_repair() {
        let manifest = Path::new("ours.json");
        assert_eq!(
            registration_state(Some(manifest), manifest, false, false),
            "missing"
        );
        assert_eq!(
            registration_state(Some(manifest), manifest, true, false),
            "stale"
        );
    }
}
