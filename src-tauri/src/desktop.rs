// 桌面集成：应用自行维护用户级 .desktop（名称/图标/右键菜单），
// 让“程序列表”与 Dock 中的名称、图标跟随配置变化。
//
// GNOME Wayland 下窗口本身的 set_icon/标题不会改变 Dock 身份；Dock 与应用列表
// 按应用 ID 匹配 .desktop 文件，因此这里在每次启动/保存配置时重写：
//   ~/.local/share/applications/com.leon.deepseek-harness.desktop
use std::path::PathBuf;

use crate::config::LauncherConfig;

pub const APP_ID: &str = "com.leon.deepseek-harness";
const DESKTOP_BASENAME: &str = "com.leon.deepseek-harness.desktop";

fn xdg_data_home() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share"))
        })
        .unwrap_or_default()
}

/// 本应用使用的用户级 .desktop 路径
pub fn desktop_file_path() -> PathBuf {
    xdg_data_home().join("applications").join(DESKTOP_BASENAME)
}

/// 菜单中显示的名称（优先配置 app_name，缺省 DeepSeek Harness）
pub fn display_name(cfg: &LauncherConfig) -> String {
    let name = cfg.app_name.trim();
    if name.is_empty() {
        "DeepSeek Harness".to_string()
    } else {
        name.to_string()
    }
}

fn escape_exec(path: &std::path::Path) -> String {
    // desktop Exec 字段转义：反斜杠、双引号、反引号、$、空格等
    path.to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('`', "\\`")
        .replace('$', "\\$")
        .replace(' ', "\\ ")
}

/// 程序入口（优先系统安装版，否则当前可执行文件）
fn executable() -> PathBuf {
    let sys = PathBuf::from("/usr/bin/deepseek-harness");
    if sys.exists() {
        sys
    } else {
        std::env::current_exe().unwrap_or(sys)
    }
}

/// 重写用户级 .desktop：Name=配置的应用名，Icon=desktop-icon.png 绝对路径，
/// 附带右键菜单 Actions“打开设置”。并隐藏 deb 自带入口避免重复。
pub fn sync(cfg: &LauncherConfig) {
    let data_home = xdg_data_home();
    let apps_dir = data_home.join("applications");
    let icon_file = data_home.join("deepseek-harness/desktop-icon.png");
    let exe = executable();
    let name = display_name(cfg);

    if std::fs::create_dir_all(&apps_dir).is_err() {
        return;
    }

    let content = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Version=1.0\n\
         Name={name}\n\
         Name[zh_CN]={name}\n\
         GenericName=Web app shell\n\
         Comment=Launch any website as a desktop app\n\
         Exec={exe}\n\
         Icon={icon}\n\
         Terminal=false\n\
         Categories=Utility;Network;\n\
         Keywords=web;app;shell;launcher;\n\
         StartupWMClass={APP_ID}\n\
         StartupNotify=false\n\
         Actions=settings;\n\
         \n\
         [Desktop Action settings]\n\
         Name=打开设置\n\
         Name[zh_CN]=打开设置\n\
         Comment=打开 DeepSeek Harness 设置页\n\
         Exec={exe} --config\n",
        exe = escape_exec(&exe),
        icon = icon_file.to_string_lossy(),
        name = name,
    );

    let _ = std::fs::write(desktop_file_path(), content);

    // 隐藏 deb 自带的 DeepSeekHarness.desktop（同名用户级文件优先级更高）
    if std::path::Path::new("/usr/share/applications/DeepSeekHarness.desktop").exists() {
        let _ = std::fs::write(
            apps_dir.join("DeepSeekHarness.desktop"),
            "[Desktop Entry]\nHidden=true\n",
        );
    }

    let _ = std::process::Command::new("update-desktop-database")
        .arg(apps_dir)
        .output();
}
