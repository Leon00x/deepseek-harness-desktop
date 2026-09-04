// 系统托盘：开关由配置 show_tray 控制，图标跟随“当前生效图标”
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{TrayIconBuilder, TrayIconId};
use tauri::AppHandle;

use crate::config::{IconMode, LauncherConfig};
use crate::icons;
use crate::window;

pub const TRAY_ID: &str = "deepseek-harness-tray";

/// 按配置应用托盘状态：移除旧的，按需重建。
/// 若配置 show_tray=false（或构建失败）则只确保托盘不存在。
pub fn apply(handle: &AppHandle, cfg: Option<&LauncherConfig>) {
    if handle.tray_by_id(&TrayIconId::from(TRAY_ID)).is_some() {
        handle.remove_tray_by_id(&TrayIconId::from(TRAY_ID));
    }
    let show = cfg.map(|c| c.show_tray).unwrap_or(true);
    if !show {
        return;
    }
    if let Err(e) = build(handle, cfg) {
        eprintln!("tray unavailable, continue without it: {e}");
    }
}

fn build(handle: &AppHandle, cfg: Option<&LauncherConfig>) -> Result<(), String> {
    let mode = cfg.map(|c| c.icon).unwrap_or(IconMode::Default);
    let icon = icons::effective_icon(handle, mode)?;
    let tooltip = cfg
        .map(|c| c.effective_title())
        .unwrap_or_else(|| "DeepSeek Harness".to_string());

    let open = MenuItem::with_id(handle, "open-settings", "打开设置…", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let reload = MenuItem::with_id(handle, "reload", "重新加载", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let separator = PredefinedMenuItem::separator(handle).map_err(|e| e.to_string())?;
    let quit = MenuItem::with_id(handle, "quit", "退出", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let menu = Menu::with_items(handle, &[&open, &reload, &separator, &quit])
        .map_err(|e| e.to_string())?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .tooltip(tooltip)
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open-settings" => {
                if let Err(e) = window::open_settings_window(app) {
                    eprintln!("open settings window: {e}");
                }
            }
            "reload" => window::reload_main_window(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .build(handle)
        .map(|_| ())
        .map_err(|e| format!("托盘创建失败: {e}"))
}
