# C++ 语言服务

本轮使用本机真实 clangd。已在原生 Editor 观察到未保存代码补全、确认后插入函数调用、错误标记随修正清除、F12 跳到本文件定义，以及 `std::vector` 成员补全。欢迎页保留用户已通过的版本。

## 配置和归属

- 全局 `config.json.language_server`：开关、可执行文件路径、参数、初始化与请求超时；软件设置已有对应分类。
- 编译器选择：当前文档覆盖 → 全局默认 → PATH 自动发现。C++ 标准、宏、包含目录和工作目录来自当前文档运行配置；不额外硬编码一组 LSP 参数。
- 一个 IDE 使用一个 clangd 进程，每份代码以 DocumentId 建立私有虚拟 `.cpp` URI。源文件无需落盘，也不会为了语言服务覆盖用户文件。保存／另存为后更新关联和编译参数。
- 使用 clangd 官方的 [`compilationDatabaseChanges`](https://clangd.llvm.org/extensions) 扩展发送每份文档的编译命令，包含真实工作目录及本地引号头文件的搜索目录。按实际选定的绝对编译器路径生成 query-driver 列表。
- 代码变更合并等待 120 ms，支持增量 UTF-16 同步；发出请求前立即同步。文档版本、缓冲区版本、配置代次和服务进程代次共同拦截过期结果。关闭再打开同一文档继续版本计数。
- 缺少 clangd 或初始化失败时保持编辑器可用；状态栏可查看错误并重启。关闭服务／退出时清理诊断与触发符并停止进程。运行时断线可能直到下次请求才显示失败。

## 分层

| 位置 | 作用 |
| --- | --- |
| `language_server.rs` | 可持久化配置与校验 |
| `language_server/clangd_config.rs` | 从文档配置生成独立编译命令 |
| `editor_adapter/language_service.rs` | 绑定文档、配置生效和状态 |
| `editor_adapter/language_service/process.rs` | clangd 启停、能力声明与协议初始化 |
| `editor_adapter/language_service/documents.rs` | 文档同步与版本化诊断 |
| `editor_adapter/language_service/completion.rs` | Editor 补全 provider 与替换范围 |
| `editor_adapter/language_service/signature.rs` | 参数提示 provider 和真实 LSP 请求 |
| `editor_adapter/language_service/navigation.rs` | 定义请求和目标转换 |
| `app/language_intelligence.rs` | IDE 生命周期、标签激活与打开定义文件 |

文档存储不依赖 GPUI；语言服务使用 Zed transport，但不引入完整 Project 的账号、协作和 Node 初始化流程。

## 验证与人工验收

真实协议试验记录在 `evidence/clangd-protocol-probe.json`：本机 Apple clangd 17 能处理未落盘源文件、每份文件的编译命令、补全及定义。原生截图在 `evidence/lsp/`，记录真实补全、F12 定义和标准库成员结果。

GPUI 的确定性测试调度器拒绝真实子进程 I/O 的外部线程唤醒，最初的真实进程测试因此未通过，不能作为协议失败或成功的证据。随后将自动回归限定为确定性传输 fixture，检查旧版本／旧进程响应拒绝、修改清除诊断、UTF-16 范围与 snippet；真实链路另外由 stdio 试验和原生窗口验证。运行记录分别保留在 `evidence/lsp-integration.log`、`lsp-regression.log` 和 `lsp-tests.log`。

打开 `target/Altaed LSP.app` 可继续测试，数据目录独立：

1. 新建未命名代码，定义一个函数，再输入名称前缀并选择补全，检查替换和参数占位符。
2. 输入不存在的标识符，修正后检查错误标记；中文和 emoji 可放在同一行验证位置。
3. 将光标放在函数调用上按 F12；快捷键可以在软件设置的表格中重新配置。
4. 打开 `evidence/lsp/fixtures/main.cpp`，对 `external_answer` 按 F12，应打开同目录的 `helper.hpp`。
5. 两个文档设置不同的宏与标准，检查各自诊断；另存为后再编辑其中一个，检查隔离。
6. 修改 clangd 为无效路径并应用，应显示不可用而能继续编辑；恢复路径并点击状态栏重启。

解锁后补测：F12 已从 `main.cpp` 打开 `helper.hpp` 并选中定义；无效 clangd 路径重新载入后显示不可用，恢复配置后真实补全再次返回。截图为 `native-cross-file-definition.jpg`、`native-unavailable.jpg` 与 `native-recovered.jpg`。不同文档宏的原生体验仍可继续验收；体验由用户判断，自动测试不代替通过标准。

## 当前边界

只针对 C++/clangd：补全及 snippet、推送诊断、定义跳转、函数参数提示和参数/类型行内提示。默认关闭头文件自动插入；需要附加编辑或执行命令的补全项不展示。本轮未实现 hover、重命名、格式化、code action 和引用列表。多定义目前选择首个结果。

每份代码独立分析，尚未将已打开但未保存的头文件作为其他翻译单元的 overlay，也未自动加载原工程 `compile_commands.json`／源码目录的 `.clangd`。这不是完整多文件工程支持；需要包含路径或宏时在文档运行配置中明确提供。虚拟 URI 也会使语言服务中的 `__FILE__` 与真实编译文件路径不同。Windows 内嵌工具链和对应 clangd 包装未验证。

## 2026-11-03 补全准确性与参数提示

- 默认等待 clangd 的语义解析，避免首次 preamble 尚未建立时返回文本猜测；重载分开展示并使用函数参数占位。旧配置未指定这些选项时继承集中默认值，同名高级参数优先，不改写用户原文件。大型文件的初次补全可能等待更久，可在软件设置调整请求超时。
- 补全标签显示类型／签名，匹配范围保留符号名；沿用 Editor 对 LSP `sortText` 与模糊匹配的排序。支持 list 的 `textEditText`、默认编辑范围和 snippet 格式；发现基线默认处理忽略 `textEditText` 后在适配边界修正，并加回归。
- 严格检查 UTF-16 字符边界、同一行、请求光标包含关系和 insert/replace 前缀关系。坏项被单独忽略，不让它覆盖代码或取消整个候选列表。
- 只有服务声明 resolve 时按需加载补全说明，且只接收展示字段；不在确认时追加未实现的 workspace edits。当前 Apple clangd 17 声明不支持 resolve，会直接提供说明，真实探针未将其当成 resolve 验证。
- 参数浮层复用 Zed 原生的重载导航、当前参数高亮、隐藏和取消逻辑；通过独立 SignatureHelpProvider 接入，不初始化完整 Project。默认自动显示，可在软件设置关闭，也可用可配置的 Cmd-Shift-Space（Windows 为 Ctrl-Shift-Space）打开。
- `scripts/patch_editor_baseline.py` 保存窄扩展并校验锚点，`prepare_editor_baseline.py` 在固定 revision 的隔离快照中重放；不修改原始 Zed checkout。

真实协议证据：`evidence/lsp/code-hints-protocol.json`；重复执行 `python3 scripts/verify_clangd.py --output evidence/lsp/code-hints-protocol.json`。它使用真实 clangd、中文与空格临时路径、同一行 emoji 和未落盘源文件，验证函数说明、第二参数定位及 vector 成员 snippet。确定性测试另覆盖参数 provider 的 UTF-16 请求、高亮与旧版本回复拒绝。主应用 37 项测试记录在 `evidence/code-intelligence-tests.log`。

原生资源栏已截图；自动输入检查过程中用户开始使用预览窗口，因此停止操作，未声称本轮原生参数浮层已经人工通过。可在 `target/Altaed Intelligence.app` 新建代码后输入两参数函数并调用，用逗号、Tab、Escape 和手动参数提示快捷键验收。用户正在编辑的会话不会被重置。

## 2026-11-03 输入补全与可选行内提示

输入时代码补全默认开启，软件设置中的“编辑器”分类可控制自动弹出，手动补全快捷键独立保留。标识符、成员访问和作用域触发使用真实 clangd 候选；函数 snippet 沿用 Editor 的确认与参数占位导航。

修复自动输入与 `BufferEvent::Edited` 的时序竞态：请求前先观察实际文本版本并发送变更，后续编辑通知对同一快照不再重复增加协议版本，避免丢掉当前候选。回归通过真实 Editor 按键触发请求并用 Tab 确认函数 snippet；也验证单次请求失败后继续输入可以恢复，不关闭整个语言服务。这项使用确定性协议夹具，区别于下面的真实 clangd 探针。

行内提示总开关默认关闭，开启后可分别控制参数名和推导类型。使用标准 `textDocument/inlayHint`，只在服务声明支持时请求，并通过 Editor 原生 inlay 显示。提示不进入源码、剪贴板或编译快照；不执行提示携带的命令、文本编辑或链接。clangd 对这两种提示的语义说明见[官方配置](https://clangd.llvm.org/config#inlayhints)。

请求合并等待250毫秒；代码修改立即清除旧提示，回复按文档身份、实际文本版本、配置代次和服务 epoch 校验。单次最多展示2000项，限制单项长度并拒绝越界 UTF-16 位置和多行标签。不支持或提示请求失败时显示原因，补全、诊断与编译继续可用。本轮查询整个翻译单元，尚未实现按可见区域分页。

真实协议探针：`python3 scripts/verify_clangd.py --output evidence/lsp/cpp-features-protocol.json`，验证函数说明、第二参数、vector snippet，以及同一行 emoji 后的 `auto` 类型与参数名提示。原生窗口已观察到参数名提示，设置页的编译选项也已检查排版；发现用户正在编辑后停止自动输入，不覆盖其内容。完整体验继续由用户验收。

最终本地48项测试与 release 编译通过，记录在 `evidence/cpp-features-tests.log` 和 `evidence/cpp-features-release.log`。本地应用包已更新；正在运行的旧进程需正常保存并重启后才加载修复。没有强制关闭用户的预览窗口，没有 push、Windows 构建或编译后的 cargo check/Clippy。
