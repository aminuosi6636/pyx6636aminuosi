# 混剪素材整理助手

本地桌面小工具，核心功能是自动日期命名和填写使用次数改名。列表左侧显示视频首帧，方便辨认素材。当前范围见 [CURRENT_SCOPE.md](docs/CURRENT_SCOPE.md)。

## 使用

1. 双击交付目录中的 `混剪素材整理助手.exe`。
2. 选择视频文件夹。已有和新放入的视频复制完成后会自动命名为 `2026-09-28_14-35-08.mp4`。
3. 选择视频，在“该视频用了”中填次数，点击“保存次数并改名”。填 5 后变成 `2026-09-28_14-35-08_使用次数：5.mp4`。

同一秒的文件用额外序号区分，不能覆盖；次数可以调高、调低或填 0。日期时间保持不变。软件打开时自动监听，关闭时停止。按本机日期时间命名，使用中文冒号以兼容 Windows。

列表在视频进入可视区域时读取第一帧并缓存缩略图。格式不受系统播放器支持或视频损坏时显示默认视频图标，不影响改名。

最终用户不需要 Node、Python、Rust或手动配置数据库。Windows版本依赖系统 WebView2；本机已验证存在。当前产物是未签名的便携程序，尚未在其他员工的电脑验收。

## 开发

Tauri 2 + React + TypeScript + Rust + bundled SQLite。职责分离，严格类型，schema migration。

```sh
npm ci
npm run tauri dev
npm run build
npm test
npm run test:rust
npm exec tauri -- build --no-bundle
```

开发机需要 Node、Rust、MSVC/Windows SDK。项目依赖由 package-lock.json 与 Cargo.lock 固定。开发环境不属于员工运行依赖。

数据库与日志保存在系统应用数据目录 `com.localvideolibrary.desktop`。沿用原标识以保留之前的设置与数据库。升级前用 SQLite Backup API 保存备份。原有损坏/未知/未来版本数据库不会被替换。

数据库和文件系统无法组成一个原子事务，因此改名前保存 pending 日志；完成后原子提交文件路径、次数与日志状态。失败时仅在确认提交状态后尝试无覆盖回滚，启动时恢复中断操作；状态含糊时停止并提示人工检查。

Windows 使用文件句柄改名、禁止目标替换，并阻止改名期间写入/删除共享。macOS 使用 RENAME_EXCL；当前只有 Windows 已测试和打包，不能把 Windows 产物当作 Mac 应用。

macOS 的 Apple Silicon 与 Intel 构建工作流及验收步骤见 [MAC_BUILD.md](docs/MAC_BUILD.md)。
