// DeepSeek Harness Desktop —— Linux 原生桌面客户端
// 开箱即用：无配置时自动写入默认连接（http://127.0.0.1:3080）并直接打开主窗口；
// 设置页可修改连接地址/名称/图标，通过图标右键菜单、托盘或 --config 打开。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod config;
mod desktop;
mod icons;
mod tray;
mod window;

use tauri::{AppHandle, Listener};

use config::{IconMode, LauncherConfig};
use window::SETTINGS_EVENT;

fn main() {
    // 某些 GNOME 会话（启用了 at-spi 无障碍桥）下 WebKitGTK 注册 webview 时会
    // 在 libatk-bridge 中段错误；在 GTK 初始化前关闭本应用的 ATK 桥可稳定绕过。
    // 仅在本应用内生效，不影响系统其他程序的无障碍。
    if std::env::var_os("NO_AT_BRIDGE").is_none() {
        std::env::set_var("NO_AT_BRIDGE", "1");
    }

    tauri::Builder::default()
        // 单实例：重复启动（如 .desktop 右键“打开设置”）转发到已运行实例
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            if argv.iter().any(|a| a == "--config" || a == "-c") {
                if let Err(e) = window::open_settings_window(app) {
                    eprintln!("open settings window: {e}");
                }
            }
        }))
        .setup(|app| {
            let handle = app.handle().clone();
            let settings_handle = handle.clone();
            // 远程页面（沉浸式悬浮栏 ⚙）发出的“打开设置”请求
            let _ = handle.listen(SETTINGS_EVENT, move |_| {
                if let Err(e) = window::open_settings_window(&settings_handle) {
                    eprintln!("open settings window: {e}");
                }
            });

            // 托盘可选：由配置 show_tray 控制；失败不阻止启动
            let cfg = LauncherConfig::load(&handle);
            tray::apply(&handle, cfg.as_ref());

            boot(&handle, cfg.as_ref()).map_err(|e| format!("启动失败: {e}"))?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_setup_state,
            commands::probe_favicon,
            commands::save_config,
            commands::live_set_opacity,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// 启动决策：
/// - 无配置 → 写入默认配置（DeepSeek Harness 本机地址），开箱即用直接打开主窗口
/// - 存在配置且未加 --config → 直接打开主窗口
/// - 带 --config 参数 → 额外打开配置页
fn boot(handle: &AppHandle, cfg: Option<&LauncherConfig>) -> Result<(), String> {
    let force_config = std::env::args().skip(1).any(|a| a == "--config" || a == "-c");

    // 首次运行：落盘默认配置，让“下载即用”成立，同时设置页也有可编辑的默认值
    let cfg = match cfg {
        Some(cfg) => cfg.clone(),
        None => {
            let default = LauncherConfig::default();
            default.save(handle)?;
            default
        }
    };
    let cfg = &cfg;

    if force_config {
        window::open_settings_window(handle)?;
    } else {
        window::create_main_window(handle, cfg, false)?;
    }

    // 把“当前生效图标”同步到桌面引用文件（每次启动都执行，无需手动再保存）
    let png = (cfg.icon != IconMode::Default)
        .then(|| icons::cached_png(handle))
        .flatten()
        .unwrap_or_else(|| icons::DEFAULT_ICON_PNG.to_vec());
    icons::sync_desktop_icon(&png);

    // 同步桌面入口（应用名称/图标/右键菜单随配置更新）
    desktop::sync(cfg);

    Ok(())
}

/// 保存后整体重启应用：新进程按新配置重新创建主窗口（避免就地重建窗口的坑）
pub fn relaunch(handle: &AppHandle) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("无法定位可执行文件: {e}"))?;
    // 单实例插件会拦掉“还在运行”的第二次启动；因此让新进程延迟 1 秒、
    // 等当前进程完全退出（释放单实例锁）后再真正启动。
    let sh = format!("sleep 1 && exec {}", shell_quote(&exe));
    std::process::Command::new("sh")
        .arg("-c")
        .arg(&sh)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("启动新进程失败: {e}"))?;
    std::thread::sleep(std::time::Duration::from_millis(250));
    handle.exit(0);
    Ok(())
}

/// 为 /bin/sh 的 Exec 加引号（路径含空格也安全）
fn shell_quote(path: &std::path::Path) -> String {
    format!("\"{}\"", path.to_string_lossy())
}