# Windows 构建与兼容性

当前状态：共享核心通过 `x86_64-pc-windows-msvc` 的交叉编译检查和 Clippy；完整 GPUI 应用还没有 Windows 构建成功或运行通过的证据。此页区分已经修正的代码、构建机要求和未验收事项。

## 两套编译环境

| 用途 | 工具链 | 使用位置 |
| --- | --- | --- |
| 构建 altaed 本身 | Rust 1.93 + MSVC C++ Build Tools + Windows SDK | 开发机或 Windows 构建机 |
| 编译用户的 C++ 解答 | MinGW-w64 g++ 或完整 clang++ 环境 | 最终用户的 IDE 内 |
| 提供补全和参数提示 | clangd | 最终用户的 IDE 内 |

最终用户无需安装 Visual Studio 或 MSVC 来使用发布包。当前尚未制作该发布包，也未验证其 DLL 是否齐全。根 `Cargo.lock` 固定 Rust 依赖；`.cargo/config.toml` 显式沿用 GPUI 基线的 Windows CRT 与错误类型编译选项，不能依赖外部 dependency workspace 的 Cargo 配置自动生效。

## 已修正

- 数据目录使用 `%LOCALAPPDATA%\altaed`，保留显式 `ALTAED_DATA_DIR` 覆盖；不再要求 Windows 存在 HOME。
- 默认代码字体为 Consolas，macOS 保持 Menlo。设置页重置值与文本框共用平台默认值。
- 编译器查找不重复添加 `.exe`；相对 PATH 项被解析为绝对路径，保留 clang++ 的驱动名称，不将符号链接规范化为 clang。
- Windows 优先探测可执行文件旁 `toolchains/mingw64/bin/g++.exe`，不存在时使用 PATH。用户显式文档／全局编译器配置优先。这里只实现发现规则，尚未附带或下载 MinGW。
- 编译结果运行时只给子进程增加所选编译器的 bin 目录，便于查找 MinGW 运行时 DLL，不修改应用或系统的全局 PATH。
- 新终端适配使用独立 PowerShell console、工作目录和每次启动的独立脚本记录；用 UTF-16LE EncodedCommand 传递命令。路径用 PowerShell 字面量转义，保留中文、空格和单引号，不修改执行策略。
- 编译和测试子进程单独使用 CREATE_NO_WINDOW，避免每个用例弹出控制台；交互运行使用另一适配器明确创建新终端。
- Windows release 入口使用 GUI subsystem。未验证 release 是否还依赖外部 DLL。
- 资源栏尺寸从编辑区预算中扣除，Windows 与 macOS 共用窗口布局逻辑。

## 构建机与运行命令

项目已整理为根 Cargo workspace；准备脚本不再生成外部消费者，直接准备仓库内 vendor/zed。首个完整验证目标为 **Windows x64 / MSVC**。建议在已有 Visual Studio C++ 构建环境的 Windows 构建机执行；本机的 Windows 11 虚拟机不可用，未再作为验收环境。后续可在 Windows CI 运行同一脚本，目前没有创建远端仓库或触发云端构建。

构建机需有：Git、Python 3.11+、Rust 1.93（含 rustfmt/Clippy 和目标标准库）、MSVC 的 cl/link/lib、Windows 10/11 SDK、CMake。release 的 GPUI 着色器编译还需要 SDK 的 fxc.exe；可以使用 `GPUI_FXC_PATH` 指定。C++ 行为测试及真实语言服务探针还需要可用的 C++20 编译器和 clangd。无需启动 Zed 的数据库、账号、Node 或协作服务。

从 **Visual Studio Developer PowerShell** 执行，使用短的源码目录减少依赖长路径问题：

```powershell
rustup target add x86_64-pc-windows-msvc --toolchain 1.93
./scripts/check_windows.ps1 -ZedSource <本地Zed源码目录> -Architecture x86_64
# 前述检查通过后，再包含 release 构建：
./scripts/check_windows.ps1 -ZedSource <本地Zed源码目录> -Architecture x86_64 -Release
```

脚本验证 MSVC 关键工具，固定 Zed revision `b0f54757a0f83bb5be0e66800b71c32a47065533`，应用可重复的编辑器 provider 扩展，在同一 workspace 执行 check/test/build/Clippy 与真实 clangd 探针。日志写入 `evidence/windows/`。`-Offline` 仅适用于依赖已缓存的构建机。仓库另提供 `.github/workflows/windows.yml`，在 Windows Server 2022/MSVC 环境手动执行；仅 workflow_dispatch 触发，不随首次推送自动运行，尚无远端成功记录。脚本支持选择 aarch64，但此架构和对应工具链组合尚未验证，不是已承诺的发行目标。

## Mac 可执行的有限检查

```sh
rustup target add x86_64-pc-windows-msvc --toolchain 1.93
cargo +1.93 check -p altaed-core --locked --target x86_64-pc-windows-msvc
cargo +1.93 clippy -p altaed-core --locked --target x86_64-pc-windows-msvc --no-deps -- -D warnings
```

检查直接针对正式的 altaed-core crate，不再使用临时跨目录 harness。它不会链接 GUI 程序，因而不需要 MSVC SDK；也不覆盖 GPUI、Editor、语言服务传输和真实 Windows API 的运行行为。完整应用交叉检查记录在 `evidence/windows-cross-check.log`，当前明确失败于缺少 MSVC `lib.exe`，不能当作成功构建。

## 发布前仍须完成

1. 在 Windows 构建机完成 debug、release 和真实 C++/clangd 行为检查；验证首次欢迎页、DPI 缩放、中文路径、输入法、剪贴板、快捷键及窗口尺寸。
2. 实测 PowerShell 独立运行、多终端生命周期和 Ctrl-C，确认 SDK/MinGW DLL 的查找；当前脚本转义只有自动回归，未在 Windows 启动终端。
3. 为编译与测试实现 Windows 进程树回收（Job Object 或同等方案）。目前取消只终止直接子进程，不能以 Unix 进程组回收的结果代替 Windows 验收。
4. 验证源码／sidecar／pending 恢复、覆盖保存、大小写路径、UNC/长路径以及被占用文件；现有存储测试只在 macOS 执行。
5. 明确随包 MinGW-w64 与 clangd 的版本、架构、分发许可和校验和，保留整个所需工具链及运行库；制作可重复安装包并在无开发工具的干净 Windows 环境验收。不能只打包 g++.exe。
6. macOS 与 Windows 的配置目录独立。迁移配置时不能把另一平台的编译器绝对路径当成可用工具链；内嵌工具链使用相对安装位置发现。
7. Windows 的 GPUI `set_menus` 仅保存菜单定义，不能把 macOS 菜单栏截图视为 Windows 菜单。需要补齐 Windows 应用菜单入口，并复用现有命令和快捷键配置；资源栏已有新建、打开和软件设置入口。

实现参考：固定基线 `crates/gpui_windows`、`docs/src/development/windows.md`、`.cargo/config.toml`；PowerShell 参数行为参考 [Microsoft 官方说明](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_powershell_exe?view=powershell-5.1)。
