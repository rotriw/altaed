# 软件配置与命令

## 归属和入口

软件设置是全局偏好；文档运行配置仍由每份代码独立保存。`config.json` 包含编辑器、工作台、默认工具链、C++ 语言服务、快捷键和插件全局选项。文档可独立覆盖编译器及编译参数；测试和 checker 配置仍归文档。

首次启动在应用数据目录创建 `config.json`，并显示固定 700×400、不可调整大小的三页欢迎引导：欢迎、C++ 环境、快捷键。C++ 环境可以跳过，完成后才进入 IDE；未完成时下次启动继续显示。环境检测实际编译并执行 C++20 程序，通过时显示绿色状态标识。开发启动器默认目录是项目的 `.cache/ide-session/`；独立运行使用 `storage::data_directory()`，也可通过 `ALTAED_DATA_DIR` 明确指定。应用菜单提供“显示 config.json”和“重新载入 config.json”。

设置界面“应用”会保存并立即更新快捷键、所有代码视图和相应的工作台偏好。外部编辑配置后，通过菜单重新载入，无需关闭代码。格式、版本、命令名或快捷键无效时保留原文件；启动失败会显示可读错误，运行中载入失败会继续使用原有效配置。

## 文件格式

```json
{
  "version": 1,
  "editor": {
    "font_family": "Menlo",
    "font_size": 14.0,
    "line_numbers": true,
    "soft_wrap": false
  },
  "workbench": { "documents": false, "tests": true, "output": false, "reduce_motion": false },
  "toolchain": { "compiler_path": null, "setup_complete": true },
  "language_server": {
    "enabled": true,
    "path": null,
    "arguments": ["--background-index", "--header-insertion=never", "--log=error"],
    "startup_timeout_ms": 15000,
    "request_timeout_ms": 5000
  },
  "keybindings": {
    "run.compile": "cmd-shift-b",
    "run.tests": "cmd-r",
    "run.run": "f5",
    "run.compile_and_run": "cmd-shift-r",
    "edit.definition": "f12",
    "run.cancel": ""
  },
  "extensions": {}
}
```

`setup_complete` 是引导状态，首次生成时为 `false`；上述示例表示已完成。编译器和 clangd 路径为 `null` 时从 PATH 自动查找。语言服务超时允许 1–300000 毫秒。

此处是节选；首次生成的文件包含完整命令表。快捷键缺少某项时继承命令目录默认值；空字符串表示停用。按键组合用 `-` 连接，多步序列用空格分开，例如 `ctrl-k ctrl-b`。默认值按宿主平台选择，macOS 使用 `cmd`，其他平台使用 `ctrl`。同一作用域与全局命令的重复或前缀冲突会拒绝应用。

文件、编辑、运行、视图、设置窗口与多行输入命令均在 `crates/altaed/settings/commands.json` 声明。旧 `run.terminal` 自动迁移到 `run.compile_and_run`，保留原覆盖值。只编译、运行已有产物、编译并运行与样例测试是四个独立命令。

设置页、首次生成配置和快捷键适配器共用此目录；按钮、菜单、快捷键最终分发同一个 GPUI Action，执行快照只由运行控制器创建。按钮提示中的按键来自当前生效映射。

## 代码结构

| 模块 | 职责 |
| --- | --- |
| `configuration/preferences.rs` | 全局偏好数据模型、值校验与表单转换 |
| `configuration/commands.rs` | 稳定命令身份、声明默认值与覆盖语义 |
| `configuration/schema.rs` | 设置描述、选择项、条件显示和命名空间注册 |
| `configuration/storage.rs` | 版本化 JSON、首次创建、旧版迁移、写入锁与变更检测 |
| `keymap.rs` | GPUI 按键语法校验、冲突检查与完整映射替换 |
| `atomic_file.rs` | 文档和配置共用的暂存、原子提交及失败清理 |
| `app/configuration_commands.rs` | 重新载入和定位配置的界面命令 |
| `app/startup.rs`、`app/welcome/` | 独立欢迎窗口、环境检测和首次配置 |
| `toolchain.rs` | 编译器与可执行文件查找、真实 C++20 环境检测 |
| `language_server/clangd_config.rs` | 每份文档的 clangd 编译命令 |
| `editor_adapter/language_service/` | 协议进程、内容同步、补全、诊断和跳转 |
| `app/language_intelligence.rs` | 文档／配置生命周期与跳转界面的协调 |
| `app/settings_window.rs` | 表单草稿、目标快照、校验和应用流程 |
| `app/settings_window/view.rs` | 字段、导航、正文、页头与操作区的视图组合 |
| `app/workbench/editor_pane.rs` | 代码标签、编辑器与运行命令入口 |
| `panel_style.rs` | 公共视觉 token、按钮与带动态快捷键的命令按钮 |

配置存储模块不依赖窗口上下文。`config.lock` 串行化同一配置目录下的应用实例写入；写入前检查原快照，避免旧设置窗口覆盖较新配置。首次创建通过不可覆盖的原子提交，普通保存通过原子替换。旧 `application-preferences.json` 仅在迁移边界读取并保留原文件。

设置描述允许插件注册带 `plugin.<id>.` 前缀的分类和值；文本、多行、整数、浮点、选择和开关复用同一套表单。未知插件值在保存时保留。当前具备此注册边界，尚未实现插件安装或加载运行时，网页中的提交插件是明确标注的结构示例。

## C++ 语言服务

软件设置中的“C++ 语言服务”控制开关、clangd 路径、启动参数和超时。修改后点击“应用”；外部编辑 JSON 后使用菜单重新载入。文档运行配置中的编译器、参数和工作目录也用于语言服务，保存该表单后生效。clangd 是外部程序，不随当前版本捆绑；缺失时仍可编辑和编译，状态栏显示原因，点击状态可重启。

默认 F12 转到定义，按键仍可在快捷键表修改。补全使用 Zed Editor 的触发与选择行为。首轮不提供重命名、格式化、自动头文件插入或完整多文件工程索引；需要附加编辑或命令的补全项暂不展示，避免只执行部分改动。详细边界与验收见 [LSP 实现与测试](lsp.md)。
