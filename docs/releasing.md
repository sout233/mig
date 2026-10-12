# 发布桌面版本

`.github/workflows/release.yml` 在向 GitHub 推送 tag 时自动构建并发布 Windows x64、macOS amd64 / arm64 和 Linux amd64 / arm64 版本。普通分支 push 不触发此流程，删除 tag 也不会执行构建。tag 指向的提交必须包含 workflow、`scripts/package-release.py`、`Cargo.lock` 和 `vendor/` 源码。

## 发布操作

先将准备发布的提交推送到 main，再创建并单独推送 tag，例如：

```powershell
git push origin main
git tag -a v0.1.2 -m 'Release v0.1.2'
git push origin v0.1.2
```

任意 tag 名称均可触发。建议使用 `v主版本.次版本.修订版本`；形如 `v0.2.0-rc.1` 或 `0.2.0-beta.1` 的 tag 自动标记为 prerelease，并且不会成为 Latest。其他 tag 由 GitHub 自动判定 Latest。workflow 不要求 tag 与 `Cargo.toml` 的版本相等，发布前应按项目的版本计划更新它。

## 构建与附件

所有构建使用固定的 `nightly-2026-05-05`。Windows 使用 `windows-2025`；macOS Intel 使用 `macos-15-intel`，Apple Silicon 使用 `macos-15`；Linux 分别使用 `ubuntu-24.04` 和 `ubuntu-24.04-arm`。所有 runner 与目标 CPU 架构一致，不交叉编译。`cargo test --locked` 和 `cargo build --locked --release` 按已提交的 `Cargo.lock` 解析依赖；依赖变化应先更新并提交 lockfile。unit tests 的 debug 信息在 CI 关闭，以减小链接和磁盘开销，release 配置不受影响。

Windows runner 自带 Visual Studio、Windows SDK 和 LLVM。LLVM 的 `libclang.dll` 用于 bindgen；vendored GPUI 的 release build 会使用 Windows SDK 中的 `fxc.exe` 编译 shader。缺失依赖或编译失败会直接使 job 失败。

macOS 固定使用 Xcode 16.4，并显式检查 Metal compiler 和 SDK，最低部署版本为 macOS 12.0。Linux job 安装 ALSA、字体、X11、Wayland、Vulkan 和 clang 等开发依赖。构建平台、运行要求和应用数据目录见 [平台说明](platforms.md)。

Release 附件包含：

- `mig-windows-x86_64.zip`：包含 `mig.exe`。
- `mig-macos-amd64.tar.gz` 和 `mig-macos-arm64.tar.gz`：包含 `MIG.app`。
- `mig-linux-amd64.tar.gz` 和 `mig-linux-arm64.tar.gz`：包含二进制、桌面启动项、图标和运行说明。
- `SHA256SUMS.txt`：五个压缩包的 SHA-256 校验值。各构建分别上传独立的 `.sha256` 文件，发布 job 汇总并校验，不会因合并同名文件丢失校验记录。

应用界面资源均嵌入可执行文件，无需 Node.js。macOS 应用使用 ad-hoc 签名，当前未配置 Developer ID 或 notarization。

## 权限与失败处理

流程使用 GitHub 自动提供的 `GITHUB_TOKEN`，无需配置 PAT 或额外 secret。构建 job 只有 `contents: read`，发布 job 才有 `contents: write`。所有 Action 固定到完整 commit SHA，checkout 不保留 Git 凭证。同一 tag 的运行串行执行，不取消正在发布的运行。

五个构建任务独立运行，某个平台失败不会取消其他平台，但所有平台的测试、编译、打包和校验都成功后才会创建 Release。流程先创建草稿并上传完整附件，再正式发布，同时使用 GitHub 自动生成的 release notes。此顺序也适用于启用 immutable releases 的仓库。

已经存在的 Release 不会被覆盖，重复发布同一 tag 会明确失败。构建失败且尚未创建 Release 时，可以修复环境问题后重新运行 workflow。若上传或发布失败后留下草稿，应先检查并删除该未发布草稿，再重跑；已发布的版本通过新 tag 发布修复，不移动旧 tag 或替换旧附件。

此流程不运行桌面界面验证程序；GitHub runner 的桌面和音频环境不作为交互验证依据。Rust 工具链或 Action 版本升级时，应显式更新 workflow 中的固定版本并检查构建结果。

## 参考

- [GitHub Actions 安全实践](https://docs.github.com/en/actions/reference/security/secure-use)
- [GitHub CLI 创建 Release](https://cli.github.com/manual/gh_release_create)
- [Windows runner 预装软件](https://github.com/actions/runner-images/blob/main/images/windows/Windows2025-Readme.md)
- [GitHub-hosted runner 架构与标签](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)
