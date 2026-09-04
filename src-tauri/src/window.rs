// 窗口构建与注入脚本：设置窗口（本地页面）、主窗口（用户配置的远程页面）
use std::path::PathBuf;

use tauri::webview::PageLoadEvent;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::config::{LauncherConfig, MIN_HEIGHT, MIN_WIDTH};
use crate::icons;

pub const MAIN_LABEL: &str = "main";
pub const CONFIG_LABEL: &str = "config";
/// 远程页面通过沉浸式悬浮栏发出的“打开设置”事件名
pub const SETTINGS_EVENT: &str = "open-settings";

// 与 NetEase Music Shell 相同的 Chrome UA，通过站点浏览器兼容检测
pub const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";

// WebKitGTK 未实现 requestIdleCallback，部分网页的 Chrome 代码路径需要它
const POLYFILL: &str = r#"
if (typeof window.requestIdleCallback !== 'function') {
  window.requestIdleCallback = function(cb, opts) {
    return setTimeout(function(){ cb({ didTimeout: false, timeRemaining: function(){ return 50; } }); }, 1);
  };
  window.cancelIdleCallback = function(id){ clearTimeout(id); };
}
"#;

// 沉浸模式：圆角透明窗口 + 顶部悬浮控制栏（拖动/最小化/最大化/关闭 + 设置入口）
// 圆角原理与 NetEase Music Shell 一致：透明渐变占住根 canvas 背景，
// 让 body 成为 fixed 后代的 containing block 后统一裁剪。
const IMMERSIVE_SCRIPT: &str = r#"
(function(){
  if (window.__walauncher_immersive) return; window.__walauncher_immersive = true;

  var CSS = ''
    + '#wa-corner{position:fixed;top:0;left:0;right:0;bottom:0;z-index:-2147483647;'
    + 'pointer-events:none;background:transparent;}'
    + 'html{background-color:transparent!important;'
    + 'background-image:linear-gradient(transparent,transparent)!important;'
    + 'overflow:hidden!important;}'
    + 'body{position:fixed!important;inset:0!important;margin:0!important;'
    + 'border-radius:16px!important;overflow:hidden!important;'
    + 'transform:translateZ(0)!important;}'
    + '#wa-wc{position:fixed;top:0;left:0;right:0;height:38px;z-index:2147483646;'
    + 'display:flex;align-items:center;justify-content:flex-end;padding:0 10px;gap:2px;'
    + 'pointer-events:none;opacity:0;transition:opacity .16s ease;user-select:none;'
    + '-webkit-user-select:none;'
    + 'background:linear-gradient(rgba(0,0,0,.50),rgba(0,0,0,.18));'
    + 'backdrop-filter:blur(10px);border-radius:16px 16px 0 0;'
    + 'font-family:system-ui,sans-serif;}'
    + '#wa-wc.on{pointer-events:auto;opacity:1;}'
    + '#wa-wc .wa-drag{flex:1;height:100%;}'
    + '#wa-wc button{all:unset;cursor:pointer;width:36px;height:28px;border-radius:7px;'
    + 'display:flex;align-items:center;justify-content:center;color:#fff;}'
    + '#wa-wc button:hover{background:rgba(255,255,255,.22);}'
    + '#wa-wc button.wa-close:hover{background:#e81123;}'
    + '#wa-wc svg{display:block;pointer-events:none;}';

  var GEAR = '<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg>';
  var MIN = '<svg width="11" height="11" viewBox="0 0 11 11"><path d="M1 5.5h9" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/></svg>';
  var MAX = '<svg width="10" height="10" viewBox="0 0 10 10"><rect x="1" y="1" width="8" height="8" rx="1.5" fill="none" stroke="currentColor" stroke-width="1.3"/></svg>';
  var CLOSE = '<svg width="11" height="11" viewBox="0 0 11 11"><path d="M1.5 1.5l8 8M9.5 1.5l-8 8" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/></svg>';

  function boot(){
    if (!window.__TAURI__ || !window.__TAURI__.window || !document.body) return false;
    var w = window.__TAURI__.window.getCurrentWindow();

    var style = document.createElement('style');
    style.textContent = CSS;
    document.head.appendChild(style);

    var bar = document.createElement('div');
    bar.id = 'wa-wc';
    var drag = document.createElement('div');
    drag.className = 'wa-drag';
    drag.setAttribute('data-tauri-drag-region', '');
    bar.appendChild(drag);

    function mk(svg, cls, tip, fn){
      var b = document.createElement('button');
      b.className = cls; b.title = tip; b.innerHTML = svg;
      b.addEventListener('click', function(e){ e.stopPropagation(); e.preventDefault(); fn(); });
      return b;
    }
    function openSettings(){
      try {
        window.__TAURI__.event.emit('open-settings');
      } catch (e) { /* 忽略 */ }
    }
    bar.appendChild(mk(GEAR,  'wa-gear',  '设置', openSettings));
    bar.appendChild(mk(MIN,  'wa-min',   '最小化', function(){ w.minimize(); }));
    bar.appendChild(mk(MAX,  'wa-max',   '最大化/还原', function(){ w.toggleMaximize(); }));
    bar.appendChild(mk(CLOSE,'wa-close', '关闭', function(){ w.close(); }));

    // 鼠标接近顶部 30px 时浮现
    document.addEventListener('mousemove', function(e){
      if (e.clientY <= 30) bar.classList.add('on');
    }, { passive: true });
    bar.addEventListener('mouseleave', function(){ bar.classList.remove('on'); });

    document.body.appendChild(bar);
    return true;
  }
  var tries = 0;
  var timer = setInterval(function(){ if (boot() || ++tries > 80) clearInterval(timer); }, 100);
})();
"#;

/// 打开设置窗口（本地配置页）；已存在时聚焦
pub fn open_settings_window(handle: &AppHandle) -> Result<(), String> {
    if let Some(w) = handle.get_webview_window(CONFIG_LABEL) {
        w.set_focus().map_err(|e| format!("聚焦设置窗口失败: {e}"))?;
        return Ok(());
    }
    let icon = icons::default_image()?;
    let builder = WebviewWindowBuilder::new(
        handle,
        CONFIG_LABEL,
        WebviewUrl::App(PathBuf::from("index.html")),
    )
    .title("DeepSeek Harness — 设置")
    .inner_size(940.0, 800.0)
    .min_inner_size(720.0, 600.0)
    .center()
    .icon(icon)
    .map_err(|e| format!("设置窗口图标失败: {e}"))?;
    builder
        .build()
        .map(|_| ())
        .map_err(|e| format!("创建设置窗口失败: {e}"))
}

/// 关闭设置窗口（若存在）
pub fn close_settings_window(handle: &AppHandle) {
    if let Some(w) = handle.get_webview_window(CONFIG_LABEL) {
        let _ = w.close();
    }
}

/// 创建（或重建）主窗口，加载用户配置的网址
pub fn create_main_window(
    handle: &AppHandle,
    cfg: &LauncherConfig,
    recreate: bool,
) -> Result<WebviewWindow, String> {
    if let Some(old) = handle.get_webview_window(MAIN_LABEL) {
        if !recreate {
            return Err("主窗口已存在".into());
        }
        let _ = old.destroy();
    }

    let url = cfg.parse_url()?;
    let icon = icons::effective_icon(handle, cfg.icon)?;
    let title = cfg.effective_title();

    let mut builder = WebviewWindowBuilder::new(
        handle,
        MAIN_LABEL,
        WebviewUrl::External(url),
    )
    .title(title)
    .inner_size(f64::from(cfg.width), f64::from(cfg.height))
    .min_inner_size(f64::from(MIN_WIDTH), f64::from(MIN_HEIGHT))
    .center()
    .user_agent(USER_AGENT)
    .icon(icon)
    .map_err(|e| format!("窗口图标设置失败: {e}"))?;

    // 页面加载完成后重新声明窗口图标：个别 WebKitGTK 后端会用站点 favicon
    // 覆盖显式设置的窗口图标（X11/WM 场景下会显示成网站图标而不是应用图标）
    {
        let icon_handle = handle.clone();
        let icon_mode = cfg.icon;
        builder = builder.on_page_load(move |win, payload| {
            if matches!(payload.event(), PageLoadEvent::Finished) {
                if let Ok(img) = icons::effective_icon(&icon_handle, icon_mode) {
                    let _ = win.set_icon(img);
                }
            }
        });
    }

    let mut translucent = false;
    if cfg.immersive {
        builder = builder
            .decorations(false)
            .transparent(true)
            .initialization_script(POLYFILL)
            .initialization_script(IMMERSIVE_SCRIPT);
        translucent = cfg.opacity < 100;
    } else if cfg.opacity < 100 {
        // 标准窗口 + 半透明：需要透明窗口画布
        builder = builder.transparent(true);
        translucent = true;
    }

    // 半透明：页面整体透明度 = opacity/100（让“透明板”效果可调）
    if translucent {
        let op = (cfg.opacity.clamp(10, 100) as f64) / 100.0;
        let script = format!(
            "(function(){{function f(){{var h=document.head;if(!h)return false;if(document.getElementById('__dsh_opacity__'))return true;var s=document.createElement('style');s.id='__dsh_opacity__';s.textContent='html{{background-color:transparent!important;}}body{{opacity:{op}!important;}}';h.appendChild(s);return true;}}if(!f())document.addEventListener('DOMContentLoaded',f,{{once:true}});}})();",
            op = op
        );
        builder = builder.initialization_script(script);
    }

    builder.build().map_err(|e| format!("创建主窗口失败: {e}"))
}

/// 重新加载主窗口当前页面
pub fn reload_main_window(handle: &AppHandle) {
    if let Some(w) = handle.get_webview_window(MAIN_LABEL) {
        let _ = w.eval("window.location.reload()");
    }
}
