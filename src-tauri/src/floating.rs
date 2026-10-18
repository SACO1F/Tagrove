use crate::{model::*, shell_target, AppState};
use serde_json::{json, Value};
use std::sync::{atomic::Ordering, mpsc};
use tauri::{Emitter, Manager};

pub fn open(app: &tauri::AppHandle, state: &AppState) -> Result<Value> {
    state.store()?.floating_presets()?;
    let settings = state.store()?.bootstrap()?["settings"].clone();
    let width = settings["floatingSize"]["width"]
        .as_f64()
        .unwrap_or(340.)
        .clamp(280., 900.);
    let height = settings["floatingSize"]["height"]
        .as_f64()
        .unwrap_or(460.)
        .clamp(320., 1000.);
    let topmost = settings["floatingAlwaysOnTop"].as_bool().unwrap_or(true);
    if let Some(w) = app.get_webview_window("floating") {
        w.unminimize().map_err(|e| err("WINDOW_ERROR", e))?;
        let position = w.outer_position().ok().map(|p| (p.x, p.y));
        let current = options(app)?;
        place(
            &w,
            position,
            current["width"].as_f64().unwrap_or(width),
            current["height"].as_f64().unwrap_or(height),
        )?;
        w.show().map_err(|e| err("WINDOW_ERROR", e))?;
        w.set_focus().map_err(|e| err("WINDOW_ERROR", e))?;
    } else {
        let mut builder = tauri::WebviewWindowBuilder::new(
            app,
            "floating",
            tauri::WebviewUrl::App("index.html?floating".into()),
        )
        .title("拾签 · 标签浮窗")
        .inner_size(width, height)
        .min_inner_size(280., 320.)
        .max_inner_size(900., 1000.)
        .resizable(true)
        .decorations(false)
        .transparent(true)
        .effects(
            tauri::window::EffectsBuilder::new()
                .effect(tauri::window::Effect::Acrylic)
                .build(),
        )
        .always_on_top(topmost)
        .theme(match settings["theme"].as_str() {
            Some("dark") => Some(tauri::Theme::Dark),
            Some("light") => Some(tauri::Theme::Light),
            _ => None,
        })
        .skip_taskbar(true)
        .visible(false)
        .focused(false);
        if std::env::var_os("SHIQIAN_DATA_DIR").is_some() {
            builder = builder.data_directory(state.store()?.root.join("webview"));
        }
        let w = builder.build().map_err(|e| err("WINDOW_ERROR", e))?;
        let position = settings["floatingPosition"]["x"]
            .as_i64()
            .zip(settings["floatingPosition"]["y"].as_i64())
            .and_then(|(x, y)| Some((i32::try_from(x).ok()?, i32::try_from(y).ok()?)));
        place(&w, position, width, height)?;
        w.show().map_err(|e| err("WINDOW_ERROR", e))?;
        // Windows recalculates the undecorated client frame on first show.
        // Reapply the requested client size before watching/persisting geometry.
        place(&w, position, width, height)?;
        w.set_focus().map_err(|e| err("WINDOW_ERROR", e))?;
        remember_geometry(&w, state);
    }
    let _ = app.emit("library-changed", ());
    let _ = app.emit("floating-visibility", json!({"visible": true}));
    Ok(json_ok())
}

fn place(
    window: &tauri::WebviewWindow,
    position: Option<(i32, i32)>,
    width: f64,
    height: f64,
) -> Result<()> {
    place_with_width(window, position, width, height, None)
}
fn place_with_width(
    window: &tauri::WebviewWindow,
    position: Option<(i32, i32)>,
    width: f64,
    height: f64,
    physical_width: Option<u32>,
) -> Result<()> {
    let mut monitors = window
        .available_monitors()
        .map_err(|e| err("WINDOW_ERROR", e))?;
    if let Ok(Some(primary)) = window.primary_monitor() {
        monitors.sort_by_key(|m| m.position() != primary.position());
    }
    let areas: Vec<_> = monitors
        .iter()
        .map(|m| {
            let a = m.work_area();
            crate::window_geometry::Area {
                x: a.position.x,
                y: a.position.y,
                width: a.size.width,
                height: a.size.height,
                scale: m.scale_factor(),
            }
        })
        .collect();
    let placement = if physical_width.is_some() {
        crate::window_geometry::transition(&areas, position, width, height)
    } else {
        crate::window_geometry::fit(&areas, position, width, height)
    };
    if let Some(p) = placement {
        window
            .set_position(tauri::PhysicalPosition::new(p.x, p.y))
            .map_err(|e| err("WINDOW_ERROR", e))?;
        if let Some(pixels) = physical_width {
            let scale = window.scale_factor().map_err(|e| err("WINDOW_ERROR",e))?;
            window.set_size(tauri::PhysicalSize::new(pixels, (p.height * scale).round() as u32))
                .map_err(|e| err("WINDOW_ERROR",e))?;
        } else {
            window.set_size(tauri::LogicalSize::new(p.width, p.height))
                .map_err(|e| err("WINDOW_ERROR",e))?;
        }
    }
    Ok(())
}
fn geometry(window: &tauri::WebviewWindow) -> Option<Value> {
    let scale = window.scale_factor().ok()?;
    let size = window.inner_size().ok()?.to_logical::<f64>(scale);
    let point = window.outer_position().ok()?;
    Some(
        json!({"position":{"x":point.x,"y":point.y},"size":{"width":size.width.round() as u32,"height":size.height.round() as u32}}),
    )
}
fn persist_geometry(state: &AppState, value: &Value) {
    if let Ok(mut store) = state.store() {
        let _ = store.save_settings(&json!({"key":"floatingPosition","value":value["position"]}));
        // Compact/minimized events must not overwrite the last expanded size.
        if value["size"]["height"].as_f64().is_some_and(|h| h >= 300.) {
            let (width,height) = crate::window_geometry::expanded_size(
                value["size"]["width"].as_f64().unwrap_or(340.),
                value["size"]["height"].as_f64().unwrap_or(460.));
            let _ = store.save_settings(&json!({"key":"floatingSize","value":{"width":width,"height":height}}));
        }
    }
}
pub fn save_geometry(app: &tauri::AppHandle, state: &AppState) {
    if let Some(window) = app.get_webview_window("floating") {
        if let Some(value) = geometry(&window) {
            persist_geometry(state, &value);
        }
    }
}
fn remember_geometry(window: &tauri::WebviewWindow, state: &AppState) {
    let (sender, receiver) = mpsc::channel();
    let watched = window.clone();
    window.on_window_event(move |event| {
        if matches!(
            event,
            tauri::WindowEvent::Resized(_)
                | tauri::WindowEvent::Moved(_)
                | tauri::WindowEvent::ScaleFactorChanged { .. }
        ) {
            if let Some(value) = geometry(&watched) {
                let _ = sender.send(value);
            }
        }
    });
    let state = state.clone();
    std::thread::spawn(move || {
        while let Ok(mut value) = receiver.recv() {
            let mut disconnected = false;
            loop {
                match receiver.recv_timeout(std::time::Duration::from_millis(250)) {
                    Ok(next) => value = next,
                    Err(mpsc::RecvTimeoutError::Timeout) => break,
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
            persist_geometry(&state, &value);
            if disconnected {
                break;
            }
        }
    });
}

pub fn visibility(app: &tauri::AppHandle) -> Value {
    json!({"visible": app.get_webview_window("floating").is_some_and(|w| w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false))})
}

pub fn options(app: &tauri::AppHandle) -> Result<Value> {
    let window = app
        .get_webview_window("floating")
        .ok_or_else(|| err("WINDOW_ERROR", "浮窗未打开"))?;
    let scale = window.scale_factor().map_err(|e| err("WINDOW_ERROR", e))?;
    let size = window
        .inner_size()
        .map_err(|e| err("WINDOW_ERROR", e))?
        .to_logical::<f64>(scale);
    let position = window
        .outer_position()
        .map_err(|e| err("WINDOW_ERROR", e))?;
    Ok(
        json!({"x":position.x,"y":position.y,"width":size.width,"height":size.height,"collapsed":size.height<100.,"minimized":window.is_minimized().map_err(|e|err("WINDOW_ERROR",e))?,"alwaysOnTop":window.is_always_on_top().map_err(|e| err("WINDOW_ERROR",e))?,"resizable":window.is_resizable().map_err(|e| err("WINDOW_ERROR",e))?}),
    )
}

pub fn resize(app: &tauri::AppHandle, state: &AppState, collapsed: bool) -> Result<Value> {
    let window = app
        .get_webview_window("floating")
        .ok_or_else(|| err("WINDOW_ERROR", "浮窗未打开"))?;
    let current = options(app)?;
    if collapsed && current["height"].as_f64().unwrap_or_default() >= 300. {
        let (width,height) = crate::window_geometry::expanded_size(
            current["width"].as_f64().unwrap_or(340.),
            current["height"].as_f64().unwrap_or(460.));
        state.store()?.save_settings(&json!({"key":"floatingSize","value":{"width":width,"height":height}}))?;
    }
    let settings = state.store()?.bootstrap()?["settings"].clone();
    let height = settings["floatingSize"]["height"].as_f64().unwrap_or(460.);
    // Apply Windows frame/constraints as one UI-thread operation. The API runs
    // on a blocking worker; separate queued setters could expose old geometry
    // to the subsequent library-changed refresh.
    let (sender, receiver) = mpsc::sync_channel(1);
    app.run_on_main_thread(move || {
        let result = (|| -> Result<()> {
            let fail = |e| err("WINDOW_ERROR", e);
            // Capture client pixels before changing frame flags. Persisted sizes
            // intentionally normalize to >=280, but must not widen a live panel.
            let pixels = window.inner_size().map_err(fail)?.width;
            let scale = window.scale_factor().map_err(fail)?;
            let width = pixels as f64 / scale;
            let position = window.outer_position().ok().map(|p| (p.x,p.y));
            window.set_min_size(None::<tauri::LogicalSize<f64>>).map_err(fail)?;
            window.set_resizable(!collapsed).map_err(fail)?;
            window.set_min_size(Some(tauri::LogicalSize::new(280., if collapsed {64.} else {320.}))).map_err(fail)?;
            let target_height = if collapsed {64.} else {height};
            place_with_width(&window,position,width,target_height,Some(pixels))?;
            Ok(())
        })();
        let _ = sender.send(result);
    }).map_err(|e|err("WINDOW_ERROR",e))?;
    receiver.recv_timeout(std::time::Duration::from_secs(3))
        .map_err(|_|err("WINDOW_ERROR","浮窗尺寸切换超时，请重试"))??;
    let actual = options(app)?;
    if actual["collapsed"].as_bool() != Some(collapsed) {
        return Err(err("WINDOW_ERROR","浮窗尺寸尚未切换，请重试"));
    }
    save_geometry(app, state);
    Ok(actual)
}

pub fn topmost(app: &tauri::AppHandle, state: &AppState, value: bool) -> Result<Value> {
    let window = app
        .get_webview_window("floating")
        .ok_or_else(|| err("WINDOW_ERROR", "浮窗未打开"))?;
    window
        .set_always_on_top(value)
        .map_err(|e| err("WINDOW_ERROR", e))?;
    state
        .store()?
        .save_settings(&json!({"key":"floatingAlwaysOnTop","value":value}))?;
    Ok(json_ok())
}

pub fn sync_theme(app: &tauri::AppHandle, theme: &str) -> Result<Value> {
    let theme = match theme {
        "dark" => tauri::Theme::Dark,
        "light" => tauri::Theme::Light,
        _ => return Err(err("INVALID_INPUT", "无效的窗口主题")),
    };
    for window in app.webview_windows().values() {
        window
            .set_theme(Some(theme))
            .map_err(|e| err("WINDOW_ERROR", e))?;
    }
    Ok(json_ok())
}
pub fn main_show(app: &tauri::AppHandle) -> Result<Value> {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
    Ok(json_ok())
}
pub fn close(app: &tauri::AppHandle, state: &AppState) -> Result<Value> {
    save_geometry(app, state);
    state.drag_cancel.store(true, Ordering::Relaxed);
    if let Some(main) = app.get_webview_window("main") {
        if !main.is_visible().unwrap_or(false) {
            main_show(app)?;
        }
    }
    if let Some(w) = app.get_webview_window("floating") {
        w.hide().map_err(|e| err("WINDOW_ERROR", e))?;
    }
    let _ = app.emit("floating-visibility", json!({"visible": false}));
    Ok(json_ok())
}
pub fn finish(app: &tauri::AppHandle, result: Result<Value>) {
    let payload = match result {
        Ok(v) => json!({"ok":true,"result":v}),
        Err(e) => json!({"ok":false,"error":e}),
    };
    let _ = app.emit("annotation-result", payload);
    let _ = app.emit("tag-drag-end", ());
    let _ = app.emit("library-changed", ());
}
pub fn start_drag(app: &tauri::AppHandle, state: &AppState, tag_id: &str) -> Result<Value> {
    if !state.store()?.tags()?.iter().any(|t| t.id == tag_id) {
        return Err(err("TAG_NOT_FOUND", "标签已删除"));
    }
    if state.drag_active.swap(true, Ordering::SeqCst) {
        return Err(err("DRAG_BUSY", "正在拖动标签"));
    }
    state.drag_cancel.store(false, Ordering::Relaxed);
    let app = app.clone();
    let state = state.clone();
    let tid = tag_id.to_owned();
    std::thread::spawn(move || {
        #[cfg(windows)]
        {
            use windows::Win32::{
                Foundation::POINT,
                UI::{
                    Input::KeyboardAndMouse::{GetAsyncKeyState, VK_ESCAPE, VK_LBUTTON},
                    WindowsAndMessaging::{GetAncestor, GetCursorPos, WindowFromPoint, GA_ROOT},
                },
            };
            let started = std::time::Instant::now();
            loop {
                if state.drag_cancel.load(Ordering::Relaxed)
                    || started.elapsed().as_secs() > 60
                    || unsafe { GetAsyncKeyState(VK_ESCAPE.0 as i32) } < 0
                {
                    let _ = app.emit("tag-drag-end", ());
                    break;
                }
                let mut point = POINT::default();
                if unsafe { GetCursorPos(&mut point) }.is_err() {
                    finish(&app, Err(err("DRAG_FAILED", "无法读取鼠标位置")));
                    break;
                }
                let down = unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) } < 0;
                let mut over_main = false;
                if let Some(w) = app.get_webview_window("main") {
                    if let (Ok(hwnd), Ok(origin), Ok(scale)) =
                        (w.hwnd(), w.inner_position(), w.scale_factor())
                    {
                        let root = unsafe { GetAncestor(WindowFromPoint(point), GA_ROOT) };
                        over_main = root.0 == hwnd.0;
                        let _=w.emit("tag-drag-point",json!({"tagId":tid,"x":(point.x-origin.x) as f64/scale,"y":(point.y-origin.y) as f64/scale,"over":over_main,"drop":!down}));
                    }
                }
                if !down {
                    if !over_main {
                        let result =
                            shell_target::file_at_point(point.x, point.y).and_then(|path| {
                                state
                                    .store()?
                                    .annotate(&json!({"tagId":tid,"paths":[path]}))
                            });
                        finish(&app, result);
                    } else {
                        let _ = app.emit("tag-drag-end", ());
                    }
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(32));
            }
        }
        #[cfg(not(windows))]
        finish(&app, Err(err("UNSUPPORTED", "请把文件拖到浮窗标签上")));
        state.drag_active.store(false, Ordering::SeqCst);
    });
    Ok(json_ok())
}
