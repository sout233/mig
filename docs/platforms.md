# 平台支持与运行

Release CI 原生构建以下平台，不依赖 CPU 模拟：

| 平台 | 架构 | Rust target | Release 附件 |
| --- | --- | --- | --- |
| Windows | amd64 / x86_64 | x86_64-pc-windows-msvc | mig-windows-x86_64.zip |
| macOS | amd64 / Intel | x86_64-apple-darwin | mig-macos-amd64.tar.gz |
| macOS | arm64 / Apple Silicon | aarch64-apple-darwin | mig-macos-arm64.tar.gz |
| Linux | amd64 / x86_64 | x86_64-unknown-linux-gnu | mig-linux-amd64.tar.gz |
| Linux | arm64 / aarch64 | aarch64-unknown-linux-gnu | mig-linux-arm64.tar.gz |

## Windows

解压后运行 `mig.exe`。运行环境需要 Microsoft Visual C++ x64 Runtime，压缩包不包含运行库安装程序。

## macOS

构建的最低系统版本为 macOS 12.0。选择对应架构的压缩包，解压后将 `MIG.app` 放入 Applications。应用图标、Info.plist 和可执行文件已打包，音频和图形使用系统 framework，不依赖单独安装 Node.js。

CI 使用 ad-hoc 签名以保证应用包的代码签名完整性；当前没有 Developer ID 签名或 Apple notarization，因此下载后首次打开可能需要按系统提示允许打开应用。

## Linux

二进制在 Ubuntu 24.04 上构建，依赖 glibc 2.39 或更新版本，以及兼容的系统共享库。它不是静态二进制或 AppImage。解压后，在包目录运行 `./mig`。桌面需要可用的 Vulkan 驱动、音频设备、D-Bus 会话，以及 X11 或 Wayland。

Ubuntu 24.04 可安装运行依赖：

```bash
sudo apt-get install libasound2t64 libfontconfig1 libfreetype6 libglib2.0-0t64 \
  libxkbcommon0 libxkbcommon-x11-0 libwayland-client0 libwayland-cursor0 \
  libx11-xcb1 libxcb1 libxcb-randr0 libxcb-shape0 libxcb-xfixes0 \
  libegl1 libgbm1 libvulkan1 mesa-vulkan-drivers libssl3t64 libzstd1 \
  xdg-desktop-portal
```

文件选择器通过 desktop portal 工作，桌面还需要与当前环境配套的 portal backend，例如 GNOME 或 KDE 的 backend。包内附有 `mig.desktop` 和 `mig.png`，其中 Exec 和 Icon 使用 `mig`；安装桌面启动项时应将可执行文件和图标安装到对应的应用路径。

## 数据保存与快捷键

三个平台均使用持久化应用数据目录：

- Windows：`%APPDATA%\MIG Music`。
- macOS：`~/Library/Application Support/MIG Music`。
- Linux：`$XDG_DATA_HOME/MIG Music`，未设置 XDG_DATA_HOME 时按 XDG 规范使用 `~/.local/share/MIG Music`。

`MIG_MUSIC_DATA_DIR` 仍可显式指定数据目录。旧版在 macOS、Linux 上使用的是临时目录；新目录不会自动迁移该临时数据。缺少可定位的数据目录会明确报错，不会再保存到临时目录。

macOS 的导入、搜索和切歌快捷键使用 Cmd；Windows 和 Linux 使用 Ctrl。Space 播放 / 暂停，Esc 关闭全屏歌词。Windows 专用的高级 GPU 歌词模糊在另外两个平台上不可用，普通歌词效果仍可使用。

## 验证范围

CI 在各原生 runner 上执行 unit tests、release build 和打包。macOS 额外校验应用签名并输出动态链接信息。所有平台成功后才合并附件、校验 SHA-256 并发布 Release。无图形、音频设备的 CI 环境不会启动桌面界面；原生编译成功不替代实际桌面交互验证。
