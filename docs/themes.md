# 颜色主题

在“设置 → 编辑器 → 颜色主题”中选择并应用。对所有代码文档、侧栏、
测试输入、输出差异和弹出菜单生效；切换保留正在编辑的内容、光标与文档身份。
启动时从 `config.json` 的 `editor.theme` 恢复，无需联网。

首版内置 14 套：默认深色／浅色（基于 VS Code Dark+／Light+）、Monokai、
GitHub Light／Dark、Solarized Light／Dark、Dracula，以及 Horizon 官方六个变体：
Horizon、Horizon Italic、Horizon Bold、Horizon Bright、Horizon Bright Italic、
Horizon Bright Bold。配置使用稳定的小写标识，例如 `horizon-bright`。
未知标识会报告配置错误，不会静默替换用户选择。

主题使用固定上游版本，原始文件与许可证位于 `crates/altaed/assets/themes/sources/`。
`python3 scripts/convert_themes.py` 离线生成原生编辑器和网页原型共用的主题数据。
C++ 的 tree-sitter 捕获映射保留对应 TextMate 的颜色及斜体／粗体（语法透明色按编辑器背景预混合）；
两种语法系统不同，此功能不执行 VS Code 扩展或声称兼容其完整作用域系统。
界面按语义角色适配，深色分隔线适当增强。原版注释等低对比语法颜色保持原样。

底部状态栏只保留输出入口、当前状态与光标位置。编译与测试执行期间显示不同
底色和状态文字，结束后恢复主题底色。默认颜色随主题生成；在“设置 → 工作台”
可分别覆盖编译色、运行色，使用 `#RRGGBB`，清空则恢复跟随主题。文字自动选取
可读颜色。外部终端独立运行，此底栏只跟踪启动过程，不追踪终端中的整个程序生命周期。

主题选择器提供轻量代码预览；预览只读取共享主题与示例，不创建额外Editor。
主窗口右上角“设置”菜单提供主题、编译环境、快捷键入口。账号项尚未开放。
