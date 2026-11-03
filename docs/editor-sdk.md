# altaed：Zed 编辑器独立化设计

验证日期：2026-11-01。固定源码基线：`b0f54757a0f83bb5be0e66800b71c32a47065533`（本地 Zed 仓库 HEAD，不等于当前官方 main）。

2026-11-03 更新：以上是历史本地实验基线。正式 workspace 改用可公开获取的上游 `cabf189854083b8e8531e9476d531e0fc68f8e3d`，再重放仓库内 provider 补丁；不依赖未发布的本地提交。

后续产品讨论已明确：altaed 是独立 IDE，敏捷交付优先；Editor 可先集成、逐步解耦。本文保留技术调查与完整拆分路线，E2—E7 不再是开始 IDE 功能开发的强制前置。当前产品范围见 [IDE 计划](ide-plan.md)。

## 结论与证据强度

值得继续验证的路线是：**保留真正的 Zed 编辑器，把它与工作台之间的依赖方向反转，形成可被 altaed 宿主使用的原生组件。** 已证明文本模型可独立调用、完整 Editor 可由外部宿主编译和创建；原生窗口已交给用户测试。移除工作台编译依赖的完整 SDK 尚未完成。

Monaco 的关键边界是 Model、URI、Editor View 和 Provider；公开 API 单独版本化，宿主不必启动 VS Code。altaed 应借鉴这套边界，而不是仅给 Zed 换入口或重写一个简单文本框。[Monaco 官方说明](https://github.com/microsoft/monaco-editor#concepts)

本地现有 `crates/zed_editor` 使用原始 `text::Buffer`，但自己实现轻量输入和绘制，没有调用真正的 `editor::Editor`。其两个测试使用记录型 LSP 替身；测试通过不代表原始编辑器已抽出，也不代表真实语言服务连通。

## 已实际执行的实验

用户明确要求亲自测试并制定评判标准，手测范围是代码编辑、虚拟 LSP。用户于 2026-11-01 反馈：“测试完毕，看上去是可以使用的。”据此记录本轮原型验收通过；不将其扩大为真实 LSP、所有输入法、性能或完整 SDK 已通过。下面的自动检查仅是工程证据，不替代用户验收。

| 实验 | 结果 | 可以说明什么 |
| --- | --- | --- |
| 既有 `zed_editor` 的 `standalone` 测试 | 2 项通过 | 现有轻量组件的已有契约成立，不作为完整 SDK 验收 |
| 独立 workspace 的 `model-host`，仅直接依赖原始 `text` | 5 项通过 | 原始模型可供外部 Rust 工程使用 |
| `model-host` 格式检查与 Clippy `-D warnings` | 通过 | 本轮薄封装与测试通过静态检查 |
| 外部包直接依赖当前工作区的真正 `editor` | manifest 解析失败 | 当前裁剪删除了仍被引用的 `workspace.dependencies.git` |
| 固定 revision 的完整源码快照与外部宿主 `cargo check` | 通过，41.54 秒 | 真正 Editor 可以由外部包引用；仍保留工作台编译依赖，不等于 SDK 独立 |
| 固定 revision 的静态本地依赖分析 | `editor` 闭包 84 个本地包 | 当前编译边界仍包含完整工作台相关依赖 |
| 真正 Editor 的无界面双视图与补全 provider 测试 | 2 项通过 | 没有创建 Project 实例；共享内容、输入、撤销重做及宿主补全调用可用 |
| 虚拟诊断标记测试 | 2 项通过 | 标记匹配处理 Unicode 与完整 token，删除标记后为空 |
| macOS 原生测试窗口 | 用户本轮手测通过 | 用户指定范围为代码编辑与虚拟 LSP，认可原型可用 |

本轮新增实验共 9 项自动测试通过（模型 5 项、实际 Editor 2 项、模拟诊断标记 2 项）；两个实验包均通过格式与 Clippy 检查。记录分别保存在 `evidence/model-host-*.log` 与 `evidence/editor-lab-*.log`。

模型实验包括：原始撤销重做、中文／emoji UTF-16 坐标往返、非法 UTF-8 编辑拒绝且不修改版本、两个模型级视图状态共享文档并各自保留锚点和滚动行、不可变快照。它没有创建 GUI，也没有验证实际视图渲染或中文输入法。

已解析的模型依赖树没有 `gpui`、`editor`、`language`、`project`、`workspace`；仍依赖 `gpui_util` 等基础包。实验包仍引用相邻 Zed 源码路径，尚不具备发行包的可移植性。见 [源码](../experiments/model-host/src/document.rs)、[锁文件](../experiments/model-host/Cargo.lock) 和 [实际依赖树](../evidence/model-host-tree.txt)。

静态分析同时计算所有平台及可选 normal/build 依赖，不含 dev-dependencies 和外部包；完整图可作为本地源码依赖上界，不是 macOS 当前启用 feature 的解析结果、二进制大小或运行时服务数。存在 unresolved 时图不完整，不能视为上界，也不能用两份报告的数量差评价裁剪收益。固定基线中 `text/language/multi_buffer/editor` 分别为 12/39/41/84 个本地包，均无 unresolved。[固定基线报告](../evidence/dependencies-committed.json)、[工作区报告](../evidence/dependencies-working-tree.json)

## 用已有接口完成依赖反转

1. **保留编辑核心。** `Editor::for_buffer` 已接受 `Option<Entity<Project>>`，`None` 是现成的运行时入口；原始 `EditorElement` 和 `DisplayMap` 保留选择、布局、折叠、换行、命中检测等实现。`None` 不会消除 Cargo 编译依赖。
2. **沿用 provider 分工。** 原实现已有 `CompletionProvider`、`SemanticsProvider`、`CodeActionProvider` 和 `CollaborationHub`。当前返回值含 `project::Hover`、`CompletionResponse`、`ProjectTransaction` 等类型，先迁移这些纯数据与事务契约。
3. **不制造新的 crate 循环。** `CompletionProvider` 的签名使用 `Context<Editor>`。不能把 trait 原样搬入一个被 Editor 依赖的 services crate。先让与视图相关的 trait 保留在编辑器 crate，只把中立数据类型下沉；需要再拆服务协议时，先消除对具体 Editor 的引用。
4. **用宿主包装类型承载 Zed 集成。** `ProjectEditorServices(Entity<Project>)` 实现 provider；`ZedEditorItem { editor: Entity<Editor>, ... }` 实现 Workspace Item、Serializable、Followable 等接口。迁到第三个 crate 后，直接为外部类型实现外部 trait 会违反 Rust orphan rule，包装类型是必要边界。
5. **拆开初始化。** 当前 `editor::init` 把 Workspace 注册、新窗口动作和编辑器基础工厂混在一起。拆成编辑器初始化与工作台适配初始化，避免隐藏的全局 AppState 前提。
6. **清理传递依赖。** `language::File::to_proto`、buffer 协作序列化和 `fs` 的 Git 依赖也要处理。仅从 Editor manifest 删除 project/workspace 仍不足以移除 `rpc` 和 Git。CRDT 文本算法可以保留；Zed 云端 wire codec 放到可选适配层。

源码切口（路径相对 Zed 根目录）：

| 位置 | 当前耦合 | 拆分动作 |
| --- | --- | --- |
| `crates/editor/src/editor.rs:332` | 基础初始化与工作台注册混合 | `init_editor` 与宿主初始化分开 |
| `crates/editor/src/editor.rs:1932` | 构造参数包含具体 Project | 保留原入口作迁移桥接，逐步改为 providers |
| `crates/editor/src/editor.rs:26748` | provider 与 Project 实现共处 | 中立类型下沉，Project 实现移至 newtype adapter |
| `crates/editor/src/items.rs:68` | Item、协作跟随、持久化 | 宿主 Item 包装视图 |
| `crates/editor/src/display_map.rs:115` | 从 project 导入 inlay、folding、token 类型 | 纯类型先迁移；保留显示流水线 |
| `crates/language/src/buffer.rs:385` | File trait 暴露 RPC wire 类型 | 文件模型和协作 codec 分离 |

## 目标组件边界

以下是目标结构，不是已经实现的 crate 清单：

```text
altaed 宿主（窗口、文档、题目关联、本地运行、提交插件）
    ├── editor-sdk：受控的公开 API 与版本兼容层
    │      ├── editor-view：原 Editor / EditorElement / DisplayMap
    │      │       └── 视图相关 provider traits、commands、events
    │      ├── editor-model：原 language / multi_buffer / text
    │      └── editor-types：不依赖 Project / Workspace 的共享类型
    └── local-lsp-adapter：服务器生命周期、协议能力与文档同步

zed-workbench-adapter（独立可选包） ──→ editor-view
           └── Project / Workspace / 协作 / 持久化
```

`editor-view` 继续依赖 GPUI；GPUI 是渲染底座，不是 altaed 的业务工作台。第一阶段不承诺 DOM、React 或 JavaScript API。当前仓库有 `gpui_web` 示例，但不能据此断言整个 Editor 已经能在浏览器运行。

公开 API 应覆盖：

- **文档**：按 URI 唯一注册模型；版本、快照、编辑事务、撤销重做、销毁。模型生命周期独立于视图。
- **视图**：挂载／切换模型，独立的光标、选择、滚动与折叠状态，释放视图不隐式关闭共享模型。
- **能力**：按模型与语言注册补全、hover、跳转、代码动作等 provider；返回可释放的注册句柄。
- **事件**：文本变化、选择变化、诊断与宿主请求；“打开文件”“保存”“导航到 URI”交给宿主执行。
- **可靠性**：请求携带模型版本与取消信息；丢弃已过期结果；多文件 WorkspaceEdit 先验证版本与范围再提交，禁止部分应用。

第一版使用 crate / Rust API，不承诺 Rust ABI 稳定。API façade 不应直接再导出整个 Project 或 Workspace，以免内部依赖继续成为宿主契约。GPUI 类型可在明确的原生视图接口中出现。

LSP 适配层负责 initialize、能力与位置编码协商、didOpen/change/close、诊断版本、取消、重启与 shutdown。不能把 raw LSP 返回值未经处理就交给视图，也不能把补全替身测试当成真实服务器验证。

## 小步实施与独立性门槛

| 步骤 | 可审查改动 | 验收 |
| --- | --- | --- |
| E0 基线 | 固定 revision、脏工作区清单、依赖图、外部模型宿主 | 当前已完成模型与依赖实验；完整 Editor 构建仍见记录 |
| E1 原生基准宿主 | 在完整源码快照上创建真正 Editor，Project=None | 开窗、输入、撤销、共享模型双视图；建立行为基准 |
| E2 共享类型 | 迁移 display-map 与 provider 涉及的 Project DTO | 原有类型行为与渲染测试保持；不新增依赖循环 |
| E3 工作台适配 | 拆初始化和 Item，增加两个 newtype | 最小宿主不初始化 Workspace；原 Zed 适配仍能编译 |
| E4 服务反转 | Project 调用经 provider/宿主请求转发 | 自定义补全 provider 能驱动原补全 UI，无 Project 实例 |
| E5 低层脱钩 | 分离 RPC codec、文件访问、Git/调试装饰 | SDK normal/build 解析依赖不含 workspace/project/client/rpc/zed |
| E6 可搬移包 | 固定源码快照、显式 manifests、许可证与锁文件 | 在无相邻 Zed 目录的独立临时目录构建宿主 |
| E7 真正编辑体验 | 原始编辑器回归与真实 clangd | IME、多光标、撤销分组、折叠、高亮、长文件、诊断/补全/跳转/重命名通过 |

先通过 E1 得到完整 Editor 的行为基准，再逐步降低依赖。每次迁移要求原宿主与外部宿主同时回归；不要大规模删除目录后才尝试编译。Cargo feature 可在过渡期隔离工作台功能，但默认 SDK 依赖树必须无工作台路径，不能用运行时 `None` 替代这个检查。

发布提取工具应采用固定来源与显式清单/补丁，在上游 API 不匹配时失败。避免用正则批量删函数、空实现冒充成功、依赖整棵 Zed 路径或仅隐藏菜单。保留原始编辑器回归测试，增加外部 consumer 的编译与行为检查。

E7 后才可以把结果称为可用的原生编辑器原型。浏览器 SDK、其他原生 UI 框架嵌入、分发体积和上游长期升级成本都需要各自验证。

当前可交互窗口只是行为基准宿主，不表示 E2—E7 已完成。用户反馈的“全部文字不可见”已定位并修复：`gpui_platform` 默认 feature 为空，而 macOS 实现只在 `font-kit` 开启时创建 `MacTextSystem`；否则选择 `NoopTextSystem`，加载字体仍返回成功。宿主现显式启用 `gpui_platform/font-kit`，重启后已观察到标题、说明与行号恢复。这也说明纯无界面测试不足以覆盖原生渲染。

## rmjac 的参考边界

已审查 `packages/graph/src/node.rs`、`edge.rs`、`typed.rs`，以及 core 的模型、事务、outbox 边界。这些可作为类型化标识、关系与一致性处理的参考，不构成 altaed 必须复用 Graph 或后端存储的决定。不要用 GPUI EntityId 或编辑器 BufferId 替代需要持久化的文档标识。

altaed 宿主管理文档、可选题目关联与运行／提交快照，再将 URI 映射到编辑模型。`rmjac_core` 的数据库、Redis 与网络服务不应成为 IDE 或编辑器库的必需依赖；当前 outbox 也不是桌面离线双向同步协议。云端、团队与完整制题能力后续另行讨论。

## 复现与构建记录

在 altaed 根目录运行：

```sh
python3 scripts/audit_zed_dependencies.py --source ../zed/zed --revision b0f54757a0f83bb5be0e66800b71c32a47065533 --output evidence/dependencies-committed.json
python3 scripts/audit_zed_dependencies.py --source ../zed/zed --output evidence/dependencies-working-tree.json
cargo +1.93 test --manifest-path experiments/model-host/Cargo.toml --locked --offline
cargo +1.93 fmt --manifest-path experiments/model-host/Cargo.toml --check
cargo +1.93 clippy --manifest-path experiments/model-host/Cargo.toml --locked --offline --all-targets --no-deps -- -D warnings
cargo +1.93 check --manifest-path experiments/editor-host/Cargo.toml --offline
```

依赖分析要求 Python 3.11+（使用标准库 `tomllib`）。最后一条针对现有裁剪工作区，当前预期在缺失 git 继承声明处失败，见 [日志](../evidence/editor-host-check.log)。

为了区分裁剪问题与上游基线，另将上述 revision 用 `git archive` 展开到 `.cache/zed-baseline`，建立 `.cache/editor-baseline-host`，复用同一探针并复制基线锁文件及 `[patch.crates-io]`。外部 workspace 不继承依赖仓库的 patch；遗漏 patch 会错误地解析到另一版 notify。快照没有修改原 Zed 工作区。[完整基线构建日志](../evidence/editor-baseline-check.log)

可用 [准备脚本](../scripts/prepare_editor_baseline.py) 重建带 revision 命名的隔离快照（Python 3.12+），它会打印外部宿主的 manifest 路径。然后对该路径运行 `cargo +1.93 check --offline --manifest-path PATH`，或 `cargo +1.93 test --offline --manifest-path PATH --test standalone`。测试源码使用真正的 Editor 和 GPUI 测试上下文，仍不能替代原生 GUI／IME 验收。

```sh
python3 scripts/prepare_editor_baseline.py --source ../zed/zed
```

## 来源与许可标记

源码依据以本机固定 revision 为准。另核对了 [Zed 官方 macOS 构建说明](https://zed.dev/docs/development/macos)、[官方 editor manifest](https://github.com/zed-industries/zed/blob/main/crates/editor/Cargo.toml) 和 [GPUI manifest](https://github.com/zed-industries/zed/blob/main/crates/gpui/Cargo.toml)。Editor 标记 `GPL-3.0-or-later`，GPUI 标记 `Apache-2.0`；提取不会自动把 Editor 变成 Monaco 的 MIT 许可，打包时必须保留所取源码的许可与来源。
