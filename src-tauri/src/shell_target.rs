//! Resolve only real items in a visible Windows Shell view. Names are matched
//! against Shell items (including hidden extensions), never concatenated to a
//! guessed folder path. Ambiguous or virtual items are rejected.
use crate::model::*;

#[cfg(windows)]
pub struct FileTarget {
    pub path: String,
    pub bounds: windows::Win32::Foundation::RECT,
}

#[cfg(windows)]
pub fn file_at_point(x: i32, y: i32) -> Result<String> {
    target_at_point(x, y).map(|target| target.path)
}

#[cfg(windows)]
pub fn target_at_point(x: i32, y: i32) -> Result<FileTarget> {
    use windows::{
        core::Interface,
        Win32::{
            Foundation::POINT,
            System::{Com::*, Variant::VARIANT},
            UI::{Accessibility::*, Shell::*, WindowsAndMessaging::*},
        },
    };
    struct ComGuard;
    impl Drop for ComGuard {
        fn drop(&mut self) {
            unsafe {
                CoUninitialize();
            }
        }
    }
    unsafe fn text(item: &IShellItem, kind: SIGDN) -> windows::core::Result<String> {
        let p = item.GetDisplayName(kind)?;
        let value = p.to_string();
        CoTaskMemFree(Some(p.0.cast()));
        Ok(value?)
    }
    unsafe fn view(dispatch: &IDispatch) -> windows::core::Result<IShellView> {
        let provider: IServiceProvider = dispatch.cast()?;
        let browser: IShellBrowser = provider.QueryService(&SID_STopLevelBrowser)?;
        browser.QueryActiveShellView()
    }
    let run = || -> windows::core::Result<FileTarget> {
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
            let _com = ComGuard;
            let point = POINT { x, y };
            let hit = WindowFromPoint(point);
            #[cfg(test)]
            {
                let mut class = [0u16; 256];
                let n = GetClassNameW(hit, &mut class);
                eprintln!(
                    "Hit {hit:?} class {}",
                    String::from_utf16_lossy(&class[..n as usize])
                );
            }
            let shell: IShellWindows = CoCreateInstance(&ShellWindows, None, CLSCTX_ALL)?;
            let mut views = vec![];
            for i in 0..shell.Count()? {
                if let Ok(dispatch) = shell.Item(&VARIANT::from(i)) {
                    if let Ok(v) = view(&dispatch) {
                        views.push(v);
                    }
                }
            }
            let mut desktop_hwnd = 0;
            if let Ok(dispatch) = shell.FindWindowSW(
                &VARIANT::from(0i32),
                &VARIANT::default(),
                SWC_DESKTOP,
                &mut desktop_hwnd,
                SWFO_NEEDDISPATCH,
            ) {
                if let Ok(v) = view(&dispatch) {
                    views.push(v);
                }
            }
            let mut candidates = vec![];
            #[cfg(test)]
            eprintln!("Shell views: {}", views.len());
            for v in views {
                let hwnd = v.GetWindow()?;
                #[cfg(test)]
                eprintln!(
                    "View {hwnd:?}, visible {}, contains {}",
                    IsWindowVisible(hwnd).as_bool(),
                    IsChild(hwnd, hit).as_bool()
                );
                // Windows 11 can report the XAML composition host (a sibling of
                // the Shell view) for WindowFromPoint. Require the same top-level
                // window AND containment in the visible view, then verify the
                // actual UIA list item below. Hidden tabs remain excluded.
                let mut rect = windows::Win32::Foundation::RECT::default();
                let in_view = GetWindowRect(hwnd, &mut rect).is_ok()
                    && x >= rect.left
                    && x < rect.right
                    && y >= rect.top
                    && y < rect.bottom;
                let same_root = GetAncestor(hwnd, GA_ROOT) == GetAncestor(hit, GA_ROOT);
                #[cfg(test)]
                eprintln!(
                    "roots {:?} / {:?}, rect {:?}, same {}",
                    GetAncestor(hwnd, GA_ROOT),
                    GetAncestor(hit, GA_ROOT),
                    rect,
                    same_root
                );
                if !IsWindowVisible(hwnd).as_bool()
                    || !(hwnd == hit || IsChild(hwnd, hit).as_bool() || (same_root && in_view))
                {
                    continue;
                }
                candidates.push(v);
            }
            if candidates.is_empty() {
                return Err(windows::core::Error::new(
                    windows::core::HRESULT(0x80004005u32 as i32),
                    "No visible Shell view under pointer",
                ));
            }
            let automation: IUIAutomation =
                CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?;
            let walker = automation.ControlViewWalker()?;
            let mut element = automation.ElementFromPoint(point)?;
            let mut item_name = None;
            let mut bounds = windows::Win32::Foundation::RECT::default();
            for _ in 0..8 {
                let kind = element.CurrentControlType()?;
                if kind == UIA_ListItemControlTypeId || kind == UIA_DataItemControlTypeId {
                    let r = element.CurrentBoundingRectangle()?;
                    if x >= r.left && x < r.right && y >= r.top && y < r.bottom {
                        item_name = Some(element.CurrentName()?.to_string());
                        bounds = r;
                    }
                    break;
                }
                if kind == UIA_WindowControlTypeId || kind == UIA_ListControlTypeId {
                    break;
                }
                element = walker.GetParentElement(&element)?;
            }
            let name = item_name.filter(|s| !s.is_empty()).ok_or_else(|| {
                windows::core::Error::new(
                    windows::core::HRESULT(0x80004005u32 as i32),
                    "Pointer is not on a file item",
                )
            })?;
            let mut paths = std::collections::HashSet::new();
            for v in candidates {
                let folder: IFolderView2 = v.cast()?;
                let count = folder.ItemCount(SVGIO_ALLVIEW)?;
                if count > 20000 {
                    continue;
                }
                for i in 0..count {
                    let item: IShellItem = match folder.GetItem(i) {
                        Ok(item) => item,
                        Err(_) => continue,
                    };
                    if text(&item, SIGDN_NORMALDISPLAY).ok().as_deref() != Some(&name) {
                        continue;
                    }
                    if let Ok(path) = text(&item, SIGDN_FILESYSPATH) {
                        if std::path::Path::new(&path).is_file() {
                            paths.insert(path);
                        }
                    }
                }
            }
            if paths.len() != 1 {
                return Err(windows::core::Error::new(
                    windows::core::HRESULT(0x80004005u32 as i32),
                    format!("Shell item has {} filesystem matches", paths.len()),
                ));
            }
            Ok(FileTarget { path: paths.into_iter().next().unwrap(), bounds })
        }
    };
    run().map_err(|e| {
        #[cfg(test)]
        eprintln!("Shell target ({x},{y}): {e}");
        #[cfg(not(test))]
        let _ = e;
        err(
            "DROP_TARGET",
            "请把标签放到桌面或资源管理器中的具体文件上；也可以把文件拖到浮窗标签上",
        )
    })
}

#[cfg(not(windows))]
pub fn file_at_point(_: i32, _: i32) -> Result<String> {
    Err(err("UNSUPPORTED", "此系统请将文件拖到浮窗标签上"))
}
