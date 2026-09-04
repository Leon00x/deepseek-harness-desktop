// 图标解析：内置默认图标、站点 favicon 探测、本地上传解码、PNG 缓存与 dataURL
use std::io::Read;
use std::time::Duration;

use base64::Engine as _;
use image::imageops::FilterType;
use image::GenericImageView;
use tauri::{image::Image, AppHandle};

use crate::config::{
    atomic_write, icon_cache_path, normalize_http_url, IconMode, LauncherConfig,
};

/// 内置默认图标（256x256 PNG，编译期嵌入）
pub const DEFAULT_ICON_PNG: &[u8] = include_bytes!("../icons/icon.png");

const MAX_DOWNLOAD: u64 = 8 * 1024 * 1024; // 8 MiB
const MAX_SIDE: u32 = 512; // 图标缓存统一 <=512px

fn http_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(6))
        .timeout_read(Duration::from_secs(10))
        .user_agent(
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36",
        )
        .build()
}

/// 将任意受支持图片字节（png/jpeg/ico）统一转为 <=512px 的 PNG
pub fn to_png_bytes(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let img = image::load_from_memory(bytes)
        .map_err(|e| format!("无法解析图片（支持 PNG/JPEG/ICO）: {e}"))?;
    let (w, h) = img.dimensions();
    let img = if w.max(h) > MAX_SIDE {
        let scale = MAX_SIDE as f32 / w.max(h) as f32;
        let nw = ((w as f32 * scale).round() as u32).max(1);
        let nh = ((h as f32 * scale).round() as u32).max(1);
        img.resize(nw, nh, FilterType::Lanczos3)
    } else {
        img
    };
    let mut out = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .map_err(|e| format!("PNG 编码失败: {e}"))?;
    Ok(out)
}

/// 图片字节 -> tauri Image
pub fn to_tauri_image(png: &[u8]) -> Result<Image<'static>, String> {
    Image::from_bytes(png).map_err(|e| format!("图标解析失败: {e}"))
}

pub fn png_data_url(png: &[u8]) -> String {
    format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png)
    )
}

/// 内置默认图标的 tauri Image
pub fn default_image() -> Result<Image<'static>, String> {
    to_tauri_image(DEFAULT_ICON_PNG)
}

fn fetch(url: &str, agent: &ureq::Agent) -> Result<Vec<u8>, String> {
    let resp = agent
        .get(url)
        .call()
        .map_err(|e| format!("下载失败 {url}: {e}"))?;
    let mut buf = Vec::new();
    resp.into_reader()
        .take(MAX_DOWNLOAD)
        .read_to_end(&mut buf)
        .map_err(|e| format!("读取失败 {url}: {e}"))?;
    if buf.is_empty() {
        return Err(format!("空响应 {url}"));
    }
    Ok(buf)
}

/// 从 HTML 中提取 `<link rel="...icon..." href="...">` 的图标地址（尽力而为，无需完整解析）
fn extract_icon_urls(html: &str, base: &url::Url) -> Vec<String> {
    let lower = html.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut pos = 0usize;
    while let Some(start) = lower[pos..].find("<link") {
        let start = pos + start;
        let end_rel = lower[start..]
            .find('>')
            .map(|e| start + e)
            .unwrap_or(lower.len());
        let tag = &lower[start..end_rel];
        if !tag.contains("icon") {
            pos = end_rel.saturating_add(1);
            continue;
        }
        // href 提取（大小写已统一为小写）
        let href = ["href=\"", "href='"]
            .iter()
            .filter_map(|q| {
                tag.find(q).map(|i| {
                    let v = &tag[i + q.len()..];
                    let end = v.find(['"', '\'']).unwrap_or(v.len());
                    &v[..end]
                })
            })
            .next()
            .map(str::to_string);
        if let Some(h) = href {
            if let Ok(abs) = base.join(&h) {
                if matches!(abs.scheme(), "http" | "https") && !out.contains(&abs.to_string()) {
                    // apple-touch-icon 更清晰，排在普通 icon 前面
                    if tag.contains("apple-touch-icon") {
                        out.insert(0, abs.to_string());
                    } else {
                        out.push(abs.to_string());
                    }
                }
            }
        }
        pos = end_rel.saturating_add(1);
    }
    out
}

/// 自动模式：探测并下载站点图标，返回 (PNG 字节, 图标来源说明)
pub fn resolve_auto(web_url: &str) -> Result<(Vec<u8>, String), String> {
    let url = normalize_http_url(web_url)?;
    let origin = format!("{}://{}", url.scheme(), url.host_str().unwrap_or(""));
    let agent = http_agent();

    let mut candidates: Vec<String> = Vec::new();
    // 1) 页面 HTML 中的 <link rel=icon>（先精确页面、再站点根）
    let pages = [web_url.to_string(), origin.clone()];
    let mut scanned = false;
    for page in pages.iter() {
        if scanned {
            break;
        }
        if let Ok(html) = fetch(page, &agent) {
            if let Ok(text) = String::from_utf8(html) {
                candidates.extend(extract_icon_urls(&text, &url));
                scanned = true;
            }
        }
    }
    // 2) 常见兜底路径
    for p in ["/favicon.ico", "/favicon.png", "/apple-touch-icon.png"] {
        let c = format!("{origin}{p}");
        if !candidates.contains(&c) {
            candidates.push(c);
        }
    }

    for c in &candidates {
        if let Ok(bytes) = fetch(c, &agent) {
            if let Ok(png) = to_png_bytes(&bytes) {
                return Ok((png, c.clone()));
            }
        }
    }
    Err(format!("未能从 {origin} 获取图标"))
}

/// 上传图片（base64 data URL）→ PNG 字节
pub fn decode_upload(upload: &str) -> Result<Vec<u8>, String> {
    let comma = upload.find(',').ok_or("图片数据格式无效".to_string())?;
    let raw = &upload[comma + 1..];
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(raw)
        .map_err(|e| format!("图片数据解码失败: {e}"))?;
    to_png_bytes(&bytes)
}

/// 读取已缓存的图标 PNG
pub fn cached_png(handle: &AppHandle) -> Option<Vec<u8>> {
    let path = icon_cache_path(handle).ok()?;
    std::fs::read(path).ok()
}

/// 当前有效图标（缓存 > 内置默认），用于窗口图标
pub fn effective_icon(handle: &AppHandle, mode: IconMode) -> Result<Image<'static>, String> {
    if mode != IconMode::Default {
        if let Some(bytes) = cached_png(handle) {
            if let Ok(img) = to_tauri_image(&bytes) {
                return Ok(img);
            }
        }
    }
    default_image()
}

/// 将 PNG 写入缓存（应用数据目录）
pub fn cache_png(handle: &AppHandle, png: &[u8]) -> Result<(), String> {
    let dir = crate::config::base_dir(handle)?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("无法创建目录: {e}"))?;
    atomic_write(&icon_cache_path(handle)?, png)
}

pub fn clear_cache(handle: &AppHandle) {
    if let Ok(p) = icon_cache_path(handle) {
        let _ = std::fs::remove_file(p);
    }
}

/// 缩放图片到指定边长（源图小于目标时返回 None，不做放大）
fn resize_to(png: &[u8], size: u32) -> Option<Vec<u8>> {
    let img = image::load_from_memory(png).ok()?;
    let (w, h) = img.dimensions();
    if w < size && h < size {
        return None;
    }
    let scale = size as f32 / w.max(h) as f32;
    let nw = ((w as f32 * scale).round() as u32).max(1);
    let nh = ((h as f32 * scale).round() as u32).max(1);
    let img = img.resize(nw, nh, FilterType::Lanczos3);
    let mut out = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .ok()?;
    Some(out)
}

fn xdg_data_home() -> std::path::PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(|h| std::path::PathBuf::from(h).join(".local/share"))
        })
        .unwrap_or_default()
}

/// 桌面入口使用的图标文件（绝对路径，`.desktop` 的 `Icon=` 直接引用）
/// 始终存在且内容 = “当前生效图标”，避免 GNOME 图标主题缓存问题。
pub fn desktop_icon_path() -> std::path::PathBuf {
    xdg_data_home().join("deepseek-harness/desktop-icon.png")
}

/// 把当前生效图标同步到桌面（`.desktop` 直接引用的文件 + 用户 hicolor 主题）。
///
/// GNOME Wayland 下 Dock/任务栏图标来自 `.desktop` 的 `Icon=`；`set_icon` 不会
/// 改变它。把用户选择的图标写入桌面引用文件与用户 hicolor 目录（优先级高于
/// 系统目录），即可让 Dock 跟随用户上传/自动获取的图标。失败时静默忽略。
pub fn sync_desktop_icon(png: &[u8]) {
    // 1) .desktop 直接引用的文件
    let file = desktop_icon_path();
    if let Some(dir) = file.parent() {
        if std::fs::create_dir_all(dir).is_ok() {
            let _ = std::fs::write(&file, png);
        }
    }
    // 2) 用户 hicolor 图标主题（兼容其他桌面环境）
    let base = xdg_data_home().join("icons/hicolor");
    let targets: [(&str, Option<Vec<u8>>); 3] = [
        ("256x256", Some(png.to_vec())),
        ("128x128", resize_to(png, 128)),
        ("32x32", resize_to(png, 32)),
    ];
    for (sub, data) in targets {
        let Some(data) = data else { continue };
        let dir = base.join(sub).join("apps");
        if std::fs::create_dir_all(&dir).is_err() {
            continue;
        }
        let _ = std::fs::write(dir.join("deepseek-harness.png"), data);
    }
}

/// 依据配置解析图标 PNG：成功返回 (png, 来源说明)，失败返回错误信息
pub fn resolve_for_config(
    handle: &AppHandle,
    cfg: &LauncherConfig,
    upload_icon: Option<&str>,
) -> Result<(Vec<u8>, Option<String>), String> {
    match cfg.icon {
        IconMode::Default => Ok((DEFAULT_ICON_PNG.to_vec(), None)),
        IconMode::Auto => {
            let url = cfg.url.clone();
            let (png, source) = resolve_auto(&url)?;
            Ok((png, Some(source)))
        }
        IconMode::Upload => {
            if let Some(upload) = upload_icon.filter(|s| !s.is_empty()) {
                return decode_upload(upload).map(|png| (png, None));
            }
            // 未重新选择文件：复用已有缓存
            if let Some(bytes) = cached_png(handle) {
                return Ok((bytes, None));
            }
            Err("请选择一张本地图片作为图标".into())
        }
    }
}
