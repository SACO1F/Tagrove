//! Browser identities and registration rules shared by the desktop and core harness.
use crate::model::*;
use serde_json::Value;
use std::path::Path;

pub const DEVELOPMENT_EXTENSION_ID: &str = include_str!("../../browser-extension/extension-id.txt");
pub const EDGE_STORE_EXTENSION_ID: &str = "oeeppgdhbgppkjgmjdaebffdhcfgeidi";

pub fn extension_ids() -> Value {
    serde_json::json!([DEVELOPMENT_EXTENSION_ID.trim(), EDGE_STORE_EXTENSION_ID])
}
pub fn allowed_origins() -> Value {
    serde_json::json!([
        format!("chrome-extension://{}/", DEVELOPMENT_EXTENSION_ID.trim()),
        format!("chrome-extension://{}/", EDGE_STORE_EXTENSION_ID)
    ])
}
pub fn current_id_config(value: &Value) -> bool {
    value["extensionId"].as_str() == Some(DEVELOPMENT_EXTENSION_ID.trim())
        && value["extensionIds"] == extension_ids()
        && value["protocol"].as_u64() == Some(1)
}
pub fn owned_id_config(value: &Value) -> bool {
    current_id_config(value)
        || (value["extensionId"].as_str() == Some(DEVELOPMENT_EXTENSION_ID.trim())
            && value.get("extensionIds").is_none()
            && value["protocol"].as_u64() == Some(1))
}
pub fn owned_origins(value: &Value) -> bool {
    value == &allowed_origins()
        || value
            == &serde_json::json!([format!(
                "chrome-extension://{}/",
                DEVELOPMENT_EXTENSION_ID.trim()
            )])
}
pub fn authorized_origin(config: &Value, origin: &str) -> bool {
    if !owned_id_config(config) {
        return false;
    }
    origin == format!("chrome-extension://{}/", DEVELOPMENT_EXTENSION_ID.trim())
        || (current_id_config(config)
            && origin == format!("chrome-extension://{}/", EDGE_STORE_EXTENSION_ID))
}

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
    fn legacy_config_can_migrate_but_does_not_implicitly_authorize_store() {
        let config = json!({"extensionId":DEVELOPMENT_EXTENSION_ID.trim(),"protocol":1});
        assert!(owned_id_config(&config));
        assert!(!current_id_config(&config));
        assert!(authorized_origin(
            &config,
            &format!("chrome-extension://{}/", DEVELOPMENT_EXTENSION_ID.trim())
        ));
        assert!(!authorized_origin(
            &config,
            &format!("chrome-extension://{EDGE_STORE_EXTENSION_ID}/")
        ));
    }
    #[test]
    fn repaired_config_supports_both_exact_origins() {
        let config = json!({"extensionId":DEVELOPMENT_EXTENSION_ID.trim(),"extensionIds":extension_ids(),"protocol":1});
        assert!(current_id_config(&config));
        for origin in allowed_origins().as_array().unwrap() {
            assert!(authorized_origin(&config, origin.as_str().unwrap()));
        }
        for origin in [
            "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/",
            "https://oeeppgdhbgppkjgmjdaebffdhcfgeidi/",
            "chrome-extension://oeeppgdhbgppkjgmjdaebffdhcfgeidi",
            "chrome-extension://oeeppgdhbgppkjgmjdaebffdhcfgeidi/extra",
        ] {
            assert!(!authorized_origin(&config, origin));
        }
    }
    #[test]
    fn configuration_cannot_add_untrusted_extension_ids() {
        let config = json!({"extensionId":DEVELOPMENT_EXTENSION_ID.trim(),"extensionIds":[DEVELOPMENT_EXTENSION_ID.trim(),"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"],"protocol":1});
        assert!(!owned_id_config(&config));
        assert!(!authorized_origin(
            &config,
            &format!("chrome-extension://{}/", DEVELOPMENT_EXTENSION_ID.trim())
        ));
    }
    #[test]
    fn host_ownership_accepts_legacy_and_current_but_not_broader_origins() {
        assert!(owned_origins(&allowed_origins()));
        assert!(owned_origins(&json!([format!(
            "chrome-extension://{}/",
            DEVELOPMENT_EXTENSION_ID.trim()
        )])));
        assert!(!owned_origins(&json!(["chrome-extension://*/"])));
        let mut broader = allowed_origins();
        broader.as_array_mut().unwrap().push(json!(
            "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/"
        ));
        assert!(!owned_origins(&broader));
    }
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
