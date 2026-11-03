# Windows CI 验证记录

源码提交：`2638e30f3ef4b7c0027e2fb71fc7a58668a38c61`。
运行：[41570900404](https://github.com/rotriw/altaed/actions/runs/41570900404)。

使用 GitHub 官方 `windows-2022` 镜像中预装的 Visual Studio 2022 / MSVC，没有安装 Visual Studio。额外安装 Rust 1.93 约12秒；下表是上传日志中 Cargo 报告的实际耗时。

| 检查 | 结果 | 首次冷构建耗时 |
| --- | --- | --- |
| Workspace check | 通过 | 11分41秒 |
| Workspace test | 27项IDE、10项core测试通过 | 编译21分39秒；测试约17秒 |
| Windows debug exe | 通过，已生成 | 13分01秒 |
| Clippy，全部目标且拒绝警告 | 通过 | 1分46秒 |
| 真实 Windows clangd 探针 | 通过 | 验证Unicode、说明、第二参数和成员snippet |
| release优化构建 | 未完成 | 随后构建至总任务90分钟上限，运行被取消 |

因此，整个运行状态是 cancelled，不能称为整条流程绿灯；前述检查和debug产物已经真实完成。

[调试程序与日志](https://github.com/rotriw/altaed/actions/runs/41570900404/artifacts/13203999699) 约50MB压缩包，程序位于 `target/x86_64-pc-windows-msvc/debug/altaed.exe`。这是开发构建，尚未在真实Windows桌面人工验收，也没有附带MinGW-w64或clangd。

随后改进工作流：每个检查独立显示实时日志和超时；保存Cargo下载与编译缓存；关闭dev/test调试符号以减少PDB开销；release仅在手动选择时执行。用户C++解答的优化配置不受这些IDE自身构建选项影响。

后续工作流改进的运行结果应另外记录，不能把本次的耗时当成缓存后性能保证。

## PD 启动故障与修复

在 Parallels Windows 11 ARM64 中运行原 x64 调试程序，实际复现启动崩溃：
`Error creating DirectWriteTextSystem`，内部错误为系统找不到路径（os error 3）。

固定版 GPUI 的 debug 着色器使用 `CARGO_MANIFEST_DIR` 指向构建机源码，然后调用
`D3DCompileFromFile`。将 exe 下载到其他电脑后该目录不存在。补丁改为内嵌两份
HLSL 与共用头文件，调用 `D3DCompile` 从内存编译；release 保留原有预编译字节码。
Windows 调试版同时启用资源 `debug-embed`，避免字体、图标、主题依赖源码目录。

补丁首次修复又暴露了导入匹配错误：短名称 `D3DCompile` 是旧名称
`D3DCompileFromFile` 的子串，被误判为已经应用。现在匹配完整导入项，并拒绝
“替换结果是输入子串”的补丁定义。两项回归检查已加入 Windows 准备阶段；
从公开固定 revision 提取的原始文件完整应用与重复应用均已在本机验证。

最新验证运行：[41590833812](https://github.com/rotriw/altaed/actions/runs/41590833812)，
提交 `ea84126`。Windows 编译、27项IDE与10项core测试、真实clangd探针均通过；
没有运行 cargo check 或 Clippy。构建阶段用时34秒，测试阶段（包含编译）2分20秒；
缓存恢复另用2分45秒。这是缓存构建，不能与冷构建直接比较。

[修复后产物](https://github.com/rotriw/altaed/actions/runs/41590833812/artifacts/13210781174)
已下载并在 PD 中以独立 `altaed-pd-fixed` 数据目录启动。核对 Windows 文件 SHA-256
与下载产物一致：`16ec384539cfaca261700ad31f7dad8363925d2871ab1cbcdc6fe29b8d41edc8`。
进程创建“欢迎使用 altaed”主窗口且 `Responding=True`，未再出现原来的初始化崩溃。
另在 PD 验证全部18个HLSL入口从内存编译通过。

这只确认修复版启动与着色器路径；复测时截图返回黑屏，尚未完成该隔离窗口的视觉、
输入法、编辑或终端人工验收。用户原有编辑窗口和文档未改动。启动日志仍包含上游
工作台未注册命令的快捷键提示，尚未在本轮整理。
