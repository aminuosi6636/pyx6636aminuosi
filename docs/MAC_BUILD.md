# macOS 构建与交付

此项目的 macOS 版与 Windows 版共用 React、Rust、SQLite 业务代码，功能和界面一致。需要在 macOS 上编译；Windows 生成的 `.exe` 不能改后缀当作 `.app`。

## 推荐：GitHub 仓库自动构建

把项目源码放在 GitHub 仓库的根目录（根目录应直接包含 `package.json` 和 `.github/workflows/build-macos.yml`）。提交到 `main` 后会自动运行 **Build macOS app**，也可在 Actions 中手动运行。工作流分别构建 Apple Silicon (`aarch64-apple-darwin`) 和 Intel (`x86_64-apple-darwin`)，在 macOS 上运行前端、SQLite 和 Rust 测试，再生成每种架构的 `.app.zip` 与 `.dmg`。构建产物保存在该次 Actions 运行的 Artifacts 区域。网页上传步骤见 [GITHUB_UPLOAD.md](GITHUB_UPLOAD.md)。

这是临时的 ad-hoc 签名交付，便于功能验收。Tauri 配置在 `src-tauri/tauri.macos.conf.json`，不会改变 Windows 的打包方式。没有 Apple Developer ID 公证的下载文件可能被 macOS Gatekeeper 阻止首次双击；需要在 Mac 上通过系统的“隐私与安全性”界面允许打开。不要关闭 Gatekeeper。要交付给普通员工直接双击，需要再接入 Developer ID 签名和 Apple 公证。

## 在 Apple Silicon Mac 上本地构建

开发机安装 Xcode Command Line Tools、Node.js 22 和 Rust 后，在源码根目录运行：

```sh
npm ci
npm test
cargo test --locked --manifest-path src-tauri/Cargo.toml
./node_modules/.bin/tauri build --target aarch64-apple-darwin --bundles app,dmg --config src-tauri/tauri.macos.conf.json
```

安装后用户不需要 Node.js 或 Rust。生成的 `.app` 和 `.dmg` 在 `src-tauri/target/aarch64-apple-darwin/release/bundle/` 下。

## 验收

在 Mac 的测试文件夹中放入两个视频，确认复制稳定后自动改成 `YYYY-MM-DD_HH-mm-ss.扩展名`；同一秒不会互相覆盖。选中一个视频填写次数，确认硬盘文件名和列表次数同步。关闭重开后确认设置和次数保留。再用 MOV、中文路径、目标重名和文件占用场景检查失败提示。缩略图依赖 macOS WebKit 支持的编码；无法解码时应显示默认视频图标，不影响改名。

本地 Windows 检查不能视为 `.app/.dmg` 已构建或已在 Mac 实机验收；以 GitHub Actions 的 macOS 构建和实机试用结果为准。
