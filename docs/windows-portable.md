# altaed · Windows x64

解压整个文件夹后运行 `altaed.exe`，无需安装 Visual Studio 或 MSVC。
适用 Windows 10 1903 或更新版本／Windows 11 x64，图形环境需支持 Direct3D 11。

首次启动可选择已有的 g++／clang++，或跳过环境配置；之后在“设置 → 编译环境”修改。
此便携包暂不附带 MinGW-w64 或 clangd。编译 C++ 需配置本机编译器；代码补全和参数提示
需安装并配置 clangd。内嵌工具链属于后续交付增量。

默认快捷键：F9 编译、F10 运行、F11 编译并运行；在“设置 → 快捷键”可修改。
右上角设置菜单可进入颜色主题、编译环境及快捷键。主题包含 Horizon 六种变体。
源码和配置保存在本机，默认数据目录为 `%LOCALAPPDATA%\altaed`。
Competitive Companion 导入入口与本地测试可用；Codeforces 提交和登录尚未开放。

`build.json` 记录源码版本和直接 DLL 依赖，`SHA256SUMS.txt` 可用于核验文件。
构建检查与首次启动探测不代替真实桌面体验验收，尤其是 DPI、输入法、窗口吸附与多终端运行。
许可证与第三方来源见 `THIRD_PARTY_NOTICES.md` 和 `licenses/`。

Windows系统ICU依赖说明：[Microsoft ICU 文档](https://learn.microsoft.com/en-us/windows/win32/intl/international-components-for-unicode--icu-)。
