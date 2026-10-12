# 发布 Windows 版本

`.github/workflows/release.yml` 在向 GitHub 推送 tag 时自动构建并发布 Windows x64 版本。普通分支 push 不触发此流程，删除 tag 也不会执行构建。tag 指向的提交必须包含该 workflow、`Cargo.lock` 和 `vendor/` 源码。

## 发布操作

在准备发布的提交上创建并推送 tag，例如：

```powershell
git tag -a v0.1.0 -m 'Release v0.1.0'
git push origin v0.1.0
```

任意 tag 名称均可触发。建议使用 `v主版本.次版本.修订版本`；形如 `v0.2.0-rc.1` 或 `0.2.0-beta.1` 的 tag 自动标记为 prerelease，并且不会成为 Latest。其他 tag 由 GitHub 自动判定 Latest。workflow 不要求 tag 与 `Cargo.toml` 的版本相等，发布前应按项目的版本计划更新它。

## 构建与附件

构建使用 `windows-2025`、`x86_64-pc-windows-msvc` 和固定的 `nightly-2026-05-05`，与配置此流程时的本地 Rust 编译器一致。`cargo build --locked --release` 确保依赖按已提交的 `Cargo.lock` 解析；依赖变化应先更新并提交 lockfile。

Windows runner 自带 Visual Studio、Windows SDK 和 LLVM。LLVM 的 `libclang.dll` 用于 bindgen；vendored GPUI 的 release build 会使用 Windows SDK 中的 `fxc.exe` 编译 shader。缺失依赖或编译失败会直接使 job 失败。

Release 附件包含：

- `mig-windows-x86_64.zip`：包含 `mig.exe`，图标等应用资源已嵌入可执行文件，无需另外打包 assets 或 Node.js。
- `SHA256SUMS.txt`：压缩包的 SHA-256 校验值。发布 job 在上传前再次校验附件。

解压后运行 `mig.exe`。这是 MSVC 构建，运行环境需要 Microsoft Visual C++ x64 Runtime；压缩包不包含运行库安装程序。

## 权限与失败处理

流程使用 GitHub 自动提供的 `GITHUB_TOKEN`，无需配置 PAT 或额外 secret。构建 job 只有 `contents: read`，发布 job 才有 `contents: write`。所有 Action 固定到完整 commit SHA，checkout 不保留 Git 凭证。同一 tag 的运行串行执行，不取消正在发布的运行。

只有构建、打包和校验全部成功后才会创建 Release。流程先创建草稿并上传附件，再正式发布，同时使用 GitHub 自动生成的 release notes。此顺序也适用于启用 immutable releases 的仓库。

已经存在的 Release 不会被覆盖，重复发布同一 tag 会明确失败。构建失败且尚未创建 Release 时，可以修复环境问题后重新运行 workflow。若上传或发布失败后留下草稿，应先检查并删除该未发布草稿，再重跑；已发布的版本通过新 tag 发布修复，不移动旧 tag 或替换旧附件。

此流程不运行桌面界面验证程序；GitHub runner 的桌面和音频环境不作为交互验证依据。Rust 工具链或 Action 版本升级时，应显式更新 workflow 中的固定版本并检查构建结果。

## 参考

- [GitHub Actions 安全实践](https://docs.github.com/en/actions/reference/security/secure-use)
- [GitHub CLI 创建 Release](https://cli.github.com/manual/gh_release_create)
- [Windows runner 预装软件](https://github.com/actions/runner-images/blob/main/images/windows/Windows2025-Readme.md)
