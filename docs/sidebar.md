# 资源侧栏

用户已批准 `prototypes/sidebar/` 的网页原型，并要求直接实现。原生实现位于 `crates/altaed/src/app/workbench/sidebar.rs`：44px 活动栏、默认220px可拖动资源栏，32px标题和28px分组／文件行。打开文件和保留草稿独立折叠；标题提供新建／打开，活动栏底部打开现有软件设置。Cmd-B／Ctrl-B 和资源图标共用现有 ToggleDocuments 命令。默认仍按用户工作台配置决定资源栏是否展开。

选择文件或恢复草稿都以 DocumentId 查找当前会话，不依赖已失效的数组位置、文件名或路径；切换保持原编辑视图。UI 不负责存储协议。活动贡献集中描述稳定入口、图标和命令；云存储入口没有命令，提示尚未接入，不模拟登录或同步。未来真实提供者需要自己的面板、状态和存储边界，不能把云存储状态塞入 CodeDocument 身份。

资源栏、编辑器和右侧面板共享布局预算，活动栏也占预算。拖动资源栏时扣除活动栏宽度，避免指针与面板宽度偏移。当前展开／折叠和尺寸只在窗口会话内保留。

本轮沿用 DESIGN.md，没有替换已确认的视觉系统、欢迎页或右侧测试区。检查了网页与原生截图的分组、文字层次、边距、细分隔线和前景 hover；原生使用现有 Zed 图标，当前活动图标本体仍为 muted、旁边有 navy 指示线，是非阻塞差异。

独立 Impeccable finish reviewer 的结论为 **ship**，范围仅侧栏，证据为 `.impeccable/review/sidebar/web.png`、`native.png`，报告在 `evidence/sidebar/finish-review.md`。未对 Rust 使用 HTML detector；审查未独立完成深色、窄窗口截图、读屏和所有键盘行为，不能把本轮审查当成全应用可访问性验收。
