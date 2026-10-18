//! Unprivileged remote child view. Only trusted local UI can invoke this module.
use crate::{model::*, AppState};
use serde_json::{json, Value};
use tauri::{LogicalPosition, LogicalSize, Manager, WebviewBuilder, WebviewUrl};
#[derive(Default)]
struct BrowserOwner {
    session: String,
    visible: bool,
}
static OWNER: std::sync::Mutex<BrowserOwner> = std::sync::Mutex::new(BrowserOwner {
    session: String::new(),
    visible: false,
});
const LABEL: &str = "source-browser";
pub fn dispatch(
    app: &tauri::AppHandle,
    state: &AppState,
    action: &str,
    v: &Value,
) -> Result<Value> {
    let mut owner = OWNER
        .lock()
        .map_err(|_| err("BROWSER_ERROR", "网页视图正忙"))?;
    let session = v["session"].as_str().unwrap_or("");
    if action == "browser.open" && uuid::Uuid::parse_str(session).is_err() {
        return Err(err("BROWSER_ERROR", "网页会话无效"));
    }
    if action != "browser.open" && !session.is_empty() && session != owner.session {
        return Ok(json!({"inactive":true}));
    }
    if action == "browser.open" {
        if let Some(view) = app.get_webview(LABEL) {
            view.close().map_err(|e| err("BROWSER_ERROR", e))?;
        }
        let (url, root) = {
            let s = state.store()?;
            let f = s.file(str_arg(v, "id")?)?;
            let source = f
                .web_source
                .ok_or_else(|| err("INVALID_INPUT", "此文件没有网页来源"))?;
            (crate::capture::checked_url(&source.url)?, s.root.clone())
        };
        let url = reqwest::Url::parse(&url).map_err(|e| err("BROWSER_ERROR", e))?;
        let builder = WebviewBuilder::new(LABEL, WebviewUrl::External(url))
            .data_directory(root.join("web-browser"))
            .focused(false)
            .on_navigation(|url| {
                matches!(url.scheme(), "http" | "https")
                    && url.username().is_empty()
                    && url.password().is_none()
            })
            .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny)
            .on_download(|_, _| false);
        let window = app
            .get_window("main")
            .ok_or_else(|| err("BROWSER_ERROR", "主窗口不可用"))?;
        let view = window
            .add_child(
                builder,
                LogicalPosition::new(0.0, 0.0),
                LogicalSize::new(1.0, 1.0),
            )
            .map_err(|e| err("BROWSER_ERROR", e))?;
        view.hide().map_err(|e| err("BROWSER_ERROR", e))?;
        owner.session = session.into();
        owner.visible = false;
    }
    if let Some(view) = app.get_webview(LABEL) {
        match action {
            "browser.close" => {
                view.close().map_err(|e| err("BROWSER_ERROR", e))?;
                owner.session.clear();
                owner.visible = false;
                return Ok(json_ok());
            }
            "browser.bounds" => {
                if v["hidden"].as_bool().unwrap_or(false) {
                    view.hide().map_err(|e| err("BROWSER_ERROR", e))?;
                    owner.visible = false;
                } else {
                    let number = |key: &str| {
                        v[key]
                            .as_f64()
                            .filter(|n| n.is_finite())
                            .unwrap_or(0.0)
                            .clamp(0.0, 20000.0)
                    };
                    let w = number("width");
                    let h = number("height");
                    if w < 20.0 || h < 20.0 {
                        view.hide().map_err(|e| err("BROWSER_ERROR", e))?;
                        owner.visible = false;
                    } else {
                        view.set_bounds(tauri::Rect {
                            position: LogicalPosition::new(number("x"), number("y")).into(),
                            size: LogicalSize::new(w, h).into(),
                        })
                        .map_err(|e| err("BROWSER_ERROR", e))?;
                        view.show().map_err(|e| err("BROWSER_ERROR", e))?;
                        owner.visible = true;
                    }
                }
            }
            "browser.back" => view
                .eval("history.back()")
                .map_err(|e| err("BROWSER_ERROR", e))?,
            "browser.forward" => view
                .eval("history.forward()")
                .map_err(|e| err("BROWSER_ERROR", e))?,
            "browser.reload" => view
                .eval("location.reload()")
                .map_err(|e| err("BROWSER_ERROR", e))?,
            "browser.stop" => view
                .eval("window.stop()")
                .map_err(|e| err("BROWSER_ERROR", e))?,
            _ => {}
        }
        return Ok(
            json!({"url":view.url().map(|u|u.to_string()).unwrap_or_default(),"visible":owner.visible,"session":owner.session}),
        );
    }
    Ok(json_ok())
}
