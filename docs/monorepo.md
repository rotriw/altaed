# Monorepo 与 crate 边界

根 `Cargo.toml` 管理一个 workspace 和唯一 `Cargo.lock`。默认构建原生 IDE；测试和 Clippy 使用 `--workspace` 覆盖全部正式 crate。

| Crate | 职责 | 不依赖 |
| --- | --- | --- |
| `crates/altaed` | GPUI 窗口、命令／快捷键、软件设置、Zed Editor 与真实 clangd 适配、工作台 | 外部开发机项目路径 |
| `crates/altaed-core` | 文档身份、会话、持久化、原子文件操作、checker、工具链、不可变编译快照、进程与终端适配 | GPUI、Zed、窗口上下文、Node、数据库、Graph |

IDE 通过常规 Cargo path dependency 引用 core。没有用 `#[path]` 从另一个应用目录借入正式源码，也没有通过缓存目录引用业务代码。编辑器依赖位于本仓库的 `vendor/zed`；它由固定 revision 和显式补丁生成，不提交完整第三方源码。`vendor/zed-source.json` 是版本来源，`scripts/prepare_zed.py` 负责准备，`scripts/patch_editor_baseline.py` 校验并重放窄扩展。

当前 core 内保留普通模块，文档模型、运行配置与 runner 之间仍有实际联系。没有仅为行数创建大量 crate。后续边界成熟时，可以将 `altaed-editor`（原生编辑组件及 provider）、导入适配和提交服务拆为独立 crate；它们通过有类型的文档标识与不可变快照交互。云存储首先作为资源提供者，而不是替换文档身份。

## 常用命令

```sh
python3 scripts/prepare_zed.py
cargo +1.93 check --workspace --locked
cargo +1.93 test --workspace --locked
cargo +1.93 clippy --workspace --all-targets --no-deps --locked -- -D warnings
cargo +1.93 build -p altaed --locked
cargo +1.93 check -p altaed-core --target x86_64-pc-windows-msvc --locked
```

Windows 核心检查直接针对正式 crate，不再依赖临时跨目录 harness。完整 Windows GUI 构建要求见 `windows-build.md`。为复用开发缓存可以显式设置 `CARGO_TARGET_DIR`，这不参与源码依赖或仓库结构。

`experiments/` 和 `evidence/` 只在本地保留历史实验、检查记录；`target/`、`.cache/`、用户会话、草稿与第三方生成源码不纳入 Git。网页原型位于 `prototypes/`，不是运行时网页前端。
