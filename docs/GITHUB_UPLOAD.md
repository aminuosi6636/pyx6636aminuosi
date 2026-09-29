# 网页上传源码并构建 Mac 版

目标仓库：<https://github.com/aminuosi6636/pyx6636aminuosi>

1. 在 Windows 上解压 `混剪素材整理助手-GitHub上传源码.zip`。打开解压后的文件夹，确认能直接看到 `package.json`、`src`、`src-tauri` 和 `.github`。不要把 ZIP 文件本身上传到仓库。
2. 打开目标仓库的网页。空仓库可以点击 **uploading an existing file**；已有文件时点击 **Add file → Upload files**。
3. 在解压后的文件夹内按 **Ctrl+A** 选中全部内容，拖入 GitHub 上传区域，保留文件夹结构。总共不足 100 个文件，无需分批。
4. 确认网页的文件清单里有 `.github/workflows/build-macos.yml`，并且 `package.json` 在仓库根目录；提交到 **main** 分支。
5. 上传提交到 `main` 后，打开仓库 **Actions**，构建会自动开始。如果 GitHub 首次提示启用 Actions，先按页面提示启用；若没有自动开始，左侧选择 **Build macOS app**，点击 **Run workflow**，分支选 **main**，再点击绿色 **Run workflow**。
6. 等两个 Mac 任务完成，在该次运行页面的 **Artifacts** 中下载对应架构：M 系列芯片选 `aarch64-apple-darwin`；Intel 芯片选 `x86_64-apple-darwin`。压缩包里有 `.app.zip` 和 `.dmg`。

目前的 Mac 包为测试用 ad-hoc 签名，尚未经过 Apple 公证。若首次打开被系统阻止，在 Mac 的 **系统设置 → 隐私与安全性** 中允许打开；不要关闭 Gatekeeper。正式向员工分发前应加入 Apple Developer ID 签名和公证。
