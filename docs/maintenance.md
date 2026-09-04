# 维护说明

DeepSeek Harness Desktop = [WebApp Launcher](https://github.com/Leon00x/webapp-launcher) 的定制版本,
沿用同一套 Tauri 2 / WebKitGTK 实现;本文记录本项目特有的默认值与踩坑点。

## 产品定位与默认值

- **开箱即用**:首次运行无配置时,自动写入默认配置并直接打开主窗口(连接 `http://127.0.0.1:3080`),
  不强制先进设置页;需要改地址时用图标右键菜单 / 托盘 / `deepseek-harness --config`。
- 默认标题与应用名:DeepSeek Harness;默认图标:仓库 `src-tauri/icons/icon.png`(由 logo 派生)。
- 应用标识 `com.leon.deepseek-harness`,配置目录 `~/.config/com.leon.deepseek-harness/`。

## 本地开发

系统依赖与 WebApp Launcher 相同(`libwebkit2gtk-4.1-dev` + `libayatana-appindicator3-dev`,托盘打包必需):

```bash
npm ci
npm run dev
cargo check --locked --manifest-path src-tauri/Cargo.toml
npm run build   # deb + AppImage
```

## 图标与桌面集成(易踩坑)

- 应用自身维护用户级 `.desktop`(`~/.local/share/applications/com.leon.deepseek-harness.desktop`):
  每次启动/保存时同步 Name(应用名)、Icon(绝对路径 `~/.local/share/deepseek-harness/desktop-icon.png`)、
  Actions(右键「打开设置」),并隐藏 deb 自带 `DeepSeekHarness.desktop` 重复项。
- GNOME Wayland 的 Dock/程序列表图标与名称**只认 .desktop**,不认窗口 set_icon;
  改动后 Shell 有缓存 → `Alt+F2` → `r`,必要时注销。
- 上传/自动获取的图标在保存时写入上面引用的文件,重启应用后 Dock 生效。
- 远程页面窗口控制靠 ACL,URLPattern 必须写成 `https://*:*` / `http://*:*`
  (裸 `https://*` 不匹配带端口的地址,如 `http://127.0.0.1:3080`),见 `capabilities/remote.json`。
- 某些 GNOME 会话(at-spi 无障碍桥)会让 WebKitGTK 在 libatk-bridge 里段错误;
  `main.rs` 在 GTK 初始化前设置 `NO_AT_BRIDGE=1`(仅本应用)。

## 单实例

`tauri-plugin-single-instance`:重复启动(如 .desktop 右键「打开设置」)转发给已运行实例,
避免出现多个主窗口。托盘开关在设置页「系统托盘图标」,保存即移除/重建托盘。

## 发布

打 tag 触发 `.github/workflows/release.yml` 自动构建并发布 deb/AppImage:

```bash
# 同步 package.json / Cargo.toml / tauri.conf.json 的 version 后
git tag v0.1.0 && git push origin v0.1.0
```
