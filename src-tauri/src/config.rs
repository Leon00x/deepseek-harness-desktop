// 配置模型：launcher 的持久化设置（config.json）与前端提交的输入
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};
use url::Url;

pub const CONFIG_FILE_NAME: &str = "config.json";
pub const ICON_FILE_NAME: &str = "app-icon.png";
pub const CONFIG_VERSION: u32 = 1;

/// 窗口/网页图标的来源方式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum IconMode {
    /// 使用内置默认图标
    #[default]
    Default,
    /// 保存时自动探测网站 favicon
    Auto,
    /// 用户上传的本地图片（拷贝到应用数据目录）
    Upload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LauncherConfig {
    pub version: u32,
    /// 要启动的网页地址（http/https，已规范化）
    pub url: String,
    /// 窗口标题；为空时使用网址域名
    pub title: String,
    /// 应用名称（程序列表 / Dock 显示）；为空时显示 “DeepSeek Harness”
    pub app_name: String,
    pub icon: IconMode,
    pub width: u32,
    pub height: u32,
    /// 沉浸模式：无边框透明窗口 + 顶部悬浮控制栏
    pub immersive: bool,
    /// 是否显示系统托盘图标（状态栏）
    pub show_tray: bool,
    /// 窗口透明度（%）：100 不透明；<100 启用半透明窗口
    pub opacity: u8,
    /// 仅背景透明：页面根背景透明（文字/内容不透明）
    pub bg_transparent: bool,
}

impl Default for LauncherConfig {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            // 开箱即用：默认连接本机 DeepSeek Harness Web 服务
            url: "http://127.0.0.1:3080/".into(),
            title: "DeepSeek Harness Desktop - Linux".into(),
            app_name: "DeepSeek Harness Desktop - Linux".into(),
            icon: IconMode::Default,
            width: 1280,
            height: 860,
            immersive: true,
            show_tray: true,
            opacity: 100,
            bg_transparent: false,
        }
    }
}

/// 前端设置页提交的完整表单
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ConfigInput {
    pub url: String,
    pub title: String,
    pub app_name: String,
    pub icon: IconMode,
    /// icon == "upload" 时携带的图片，格式为 data:image/...;base64,...
    pub upload_icon: Option<String>,
    pub width: u32,
    pub height: u32,
    pub immersive: bool,
    pub show_tray: bool,
    pub opacity: u8,
    pub bg_transparent: bool,
}

impl ConfigInput {
    /// 校验并规范化：补齐协议、限制窗口尺寸
    pub fn normalize(&mut self) -> Result<(), String> {
        self.url = self.url.trim().to_string();
        if self.url.is_empty() {
            return Err("请填写要启动的网页地址".into());
        }
        let url = normalize_http_url(&self.url)?;
        self.url = url.to_string();
        self.title = self.title.trim().to_string();
        self.width = self.width.clamp(MIN_WIDTH, 7680);
        self.height = self.height.clamp(MIN_HEIGHT, 4320);
        self.opacity = self.opacity.clamp(10, 100);
        Ok(())
    }

    pub fn into_config(self) -> LauncherConfig {
        LauncherConfig {
            version: CONFIG_VERSION,
            url: self.url,
            title: self.title,
            app_name: self.app_name,
            icon: self.icon,
            width: self.width,
            height: self.height,
            immersive: self.immersive,
            show_tray: self.show_tray,
            opacity: self.opacity,
            bg_transparent: self.bg_transparent,
        }
    }
}

pub const MIN_WIDTH: u32 = 640;
pub const MIN_HEIGHT: u32 = 480;

/// 规范化用户输入的网址：无协议时补 https://，并校验 scheme 为 http/https
pub fn normalize_http_url(raw: &str) -> Result<Url, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("网址不能为空".into());
    }
    let s = if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };
    let url = Url::parse(&s).map_err(|_| "网址格式无效".to_string())?;
    match url.scheme() {
        "http" | "https" => Ok(url),
        _ => Err("仅支持 http / https 网页地址".into()),
    }
}

pub fn domain_of(url: &Url) -> String {
    url.host_str().unwrap_or("DeepSeek Harness").to_string()
}

impl LauncherConfig {
    pub fn effective_title(&self) -> String {
        if !self.title.trim().is_empty() {
            return self.title.trim().to_string();
        }
        match normalize_http_url(&self.url) {
            Ok(u) => domain_of(&u),
            Err(_) => "DeepSeek Harness".into(),
        }
    }

    pub fn parse_url(&self) -> Result<Url, String> {
        normalize_http_url(&self.url)
    }
}

/// 应用配置目录（`~/.config/<identifier>` 或退回数据目录）
pub fn base_dir(handle: &AppHandle) -> Result<PathBuf, String> {
    handle
        .path()
        .app_config_dir()
        .or_else(|_| handle.path().app_data_dir())
        .map_err(|e| format!("无法获取应用配置目录: {e}"))
}

pub fn config_path(handle: &AppHandle) -> Result<PathBuf, String> {
    Ok(base_dir(handle)?.join(CONFIG_FILE_NAME))
}

pub fn icon_cache_path(handle: &AppHandle) -> Result<PathBuf, String> {
    Ok(base_dir(handle)?.join(ICON_FILE_NAME))
}

impl LauncherConfig {
    pub fn load(handle: &AppHandle) -> Option<LauncherConfig> {
        let path = config_path(handle).ok()?;
        let bytes = std::fs::read(path).ok()?;
        serde_json::from_slice::<LauncherConfig>(&bytes).ok()
    }

    pub fn save(&self, handle: &AppHandle) -> Result<(), String> {
        let dir = base_dir(handle)?;
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("无法创建配置目录: {e}"))?;
        let json = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        atomic_write(&config_path(handle)?, &json)
    }
}

/// 原子写入（临时文件 + rename），避免半截文件
pub fn atomic_write(path: &std::path::Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes).map_err(|e| format!("写入失败: {e}"))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("写入失败: {e}"))
}
