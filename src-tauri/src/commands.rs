// Tauri 命令：设置页通过 invoke 与后端交互
use serde::Serialize;
use tauri::AppHandle;

use crate::config::{ConfigInput, IconMode, LauncherConfig};
use crate::icons;

/// 设置页初始化时获取当前状态
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupState {
    pub has_config: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<LauncherConfig>,
    /// 当前生效图标的 PNG dataURL（default 模式为 null）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_data_url: Option<String>,
}

#[tauri::command]
pub fn get_setup_state(app: AppHandle) -> Result<SetupState, String> {
    let Some(cfg) = LauncherConfig::load(&app) else {
        return Ok(SetupState::default());
    };
    // 无论哪种模式都返回“当前生效图标”，让设置页预览与实际一致（default=内置 logo）
    let icon_data_url = {
        let png = if cfg.icon != IconMode::Default {
            icons::cached_png(&app).unwrap_or_else(|| icons::DEFAULT_ICON_PNG.to_vec())
        } else {
            icons::DEFAULT_ICON_PNG.to_vec()
        };
        Some(icons::png_data_url(&png))
    };
    Ok(SetupState {
        has_config: true,
        config: Some(cfg),
        icon_data_url,
    })
}

/// 自动图标模式的实时探测结果（用于预览）
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeResult {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[tauri::command]
pub async fn probe_favicon(url: String) -> ProbeResult {
    let normalized = match crate::config::normalize_http_url(&url) {
        Ok(u) => u.to_string(),
        Err(e) => {
            return ProbeResult {
                ok: false,
                data_url: None,
                source: None,
                error: Some(e),
            }
        }
    };
    match tauri::async_runtime::spawn_blocking(move || icons::resolve_auto(&normalized)).await {
        Ok(Ok((png, source))) => ProbeResult {
            ok: true,
            data_url: Some(icons::png_data_url(&png)),
            source: Some(source),
            error: None,
        },
        Ok(Err(e)) => ProbeResult {
            ok: false,
            data_url: None,
            source: None,
            error: Some(e),
        },
        Err(e) => ProbeResult {
            ok: false,
            data_url: None,
            source: None,
            error: Some(format!("任务异常: {e}")),
        },
    }
}

/// 保存（可选立即启动）的结果
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveResult {
    pub saved: bool,
    pub launched: bool,
    /// 保存后实际生效图标的 PNG dataURL（default 或解析失败时为 null）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_data_url: Option<String>,
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[tauri::command]
pub async fn save_config(app: AppHandle, input: ConfigInput, launch: bool) -> SaveResult {
    let mut input = input;
    if let Err(e) = input.normalize() {
        return SaveResult {
            error: Some(e),
            ..Default::default()
        };
    }
    let cfg = input.clone().into_config();

    // ---- 图标解析（可能联网，放阻塞线程） ----
    let icon_result = {
        let handle = app.clone();
        let upload = input.upload_icon.clone();
        let cfg = cfg.clone();
        tauri::async_runtime::spawn_blocking(move || {
            icons::resolve_for_config(&handle, &cfg, upload.as_deref())
        })
        .await
    }
    .map_err(|e| format!("任务异常: {e}"))
    .and_then(|r| r);

    let mut warnings = Vec::new();
    let mut effective_png: Option<Vec<u8>> = None;
    let icon_data_url = match icon_result {
        Ok((png, _source)) => {
            effective_png = Some(png.clone());
            if cfg.icon == IconMode::Default {
                icons::clear_cache(&app);
            } else {
                if let Err(e) = icons::cache_png(&app, &png) {
                    warnings.push(format!("图标缓存失败: {e}"));
                }
            }
            Some(icons::png_data_url(&png))
        }
        Err(e) => {
            // 图标解析失败不阻塞启动：回退内置图标并提示
            if cfg.icon != IconMode::Default {
                icons::clear_cache(&app);
                warnings.push(format!("图标获取失败，已使用默认图标：{e}"));
            }
            Some(icons::png_data_url(icons::DEFAULT_ICON_PNG))
        }
    };
    // ---- 持久化配置 ----
    if let Err(e) = cfg.save(&app) {
        return SaveResult {
            error: Some(e),
            ..Default::default()
        };
    }

    // 同步桌面引用图标与 .desktop（名称/图标/右键菜单随配置即时更新），
    // 托盘开关也立即生效（移除/重建托盘）。失败不影响主流程。
    icons::sync_desktop_icon(effective_png.as_deref().unwrap_or(icons::DEFAULT_ICON_PNG));
    crate::desktop::sync(&cfg);
    crate::tray::apply(&app, Some(&cfg));

    // ---- 保存并（重新）启动：整体重启，让窗口模式/透明度/背景透明等生效 ----
    if launch {
        if let Err(e) = crate::relaunch(&app) {
            return SaveResult {
                saved: true,
                launched: false,
                warnings,
                error: Some(format!("重启失败: {e}")),
                icon_data_url,
            };
        }
    }

    SaveResult {
        saved: true,
        launched: launch,
        icon_data_url,
        warnings,
        error: None,
    }
}

/// 实时调整主窗口整体透明度（拖动滑块即时预览）。
/// 仅当主窗口画布已启用透明（沉浸/背景透明/半透明）时可见效果。
#[tauri::command]
pub fn live_set_opacity(app: tauri::AppHandle, opacity: u8) {
    use tauri::Manager;
    let Some(w) = app.get_webview_window(crate::window::MAIN_LABEL) else {
        return;
    };
    let op = (opacity.clamp(10, 100) as f64) / 100.0;
    let js = format!("(function(){{var b=document.body;if(b)b.style.opacity='{op}';}})()");
    let _ = w.eval(js);
}
