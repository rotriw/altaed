# altaed

面向竞赛编程的本地 IDE。使用原生 GPUI 与 Zed Editor，支持独立代码文档、未保存快照编译、本地测试和真实 C++ 语言服务。主要交付桌面应用；网页只用于界面原型。

## Monorepo

- `crates/altaed`：原生 IDE、工作台、设置、快捷键和编辑器／clangd 适配。
- `crates/altaed-core`：文档身份、会话、持久化、工具链、编译运行与 checker；不依赖 GPUI/Zed。
- `vendor/zed-source.json`：固定第三方 revision；准备脚本将源码和窄扩展放入忽略的 `vendor/zed`。
- `prototypes/`：已讨论的网页界面原型，不作为 IDE 运行时。

当前集成真实 clangd 补全、函数参数提示、诊断和定义导航。源文件、样例、运行配置属于各自 DocumentId。全局软件配置与文档运行配置分开。云存储、Competitive Companion 正式导入、插件远程提交与 CF 中继仍待实现。

## 构建

先安装 Rust 1.93 和对应系统的原生构建工具，然后在仓库根目录执行：

```sh
python3 scripts/prepare_zed.py
cargo +1.93 build -p altaed --locked
cargo +1.93 run -p altaed --locked
```

准备脚本需要 Git、Python 3.11.8+，会获取固定 revision，不需要相邻的 Zed checkout。可以用 `--source <已有Zed Git目录>` 离线复用归档来源；依赖准备后，Cargo 可在缓存齐全时加 `--offline`。

```sh
cargo +1.93 fmt --all -- --check
cargo +1.93 test --workspace --locked
cargo +1.93 clippy --workspace --all-targets --no-deps --locked -- -D warnings
```

macOS 需要 Xcode 及 Metal 工具。Windows 完整构建需要 MSVC Build Tools、Windows SDK 和 CMake；用户 C++ 代码使用的 MinGW-w64 是另一套工具链。Windows 共享核心已经通过目标检查，完整 GUI 与发布包尚未验证，详见 [Windows 构建说明](docs/windows-build.md)。首次欢迎页允许暂时跳过 C++ 配置。

## 文档

- [产品范围](PRODUCT.md) 与 [迭代计划](docs/ide-plan.md)
- [Crate 边界](docs/monorepo.md)
- [软件配置与快捷键](docs/configuration.md)
- [真实语言服务与验证范围](docs/lsp.md)
- [资源侧栏](docs/sidebar.md)
- [编辑器独立性研究](docs/editor-sdk.md)
- [第三方来源与许可证说明](THIRD_PARTY_NOTICES.md)

`.cache/`、`target/`、本地实验、验收截图和个人会话不纳入 Git。直接运行应用使用系统数据目录，可用 `ALTAED_DATA_DIR` 指定独立测试会话。不要将个人源码或账号会话提交到仓库。
