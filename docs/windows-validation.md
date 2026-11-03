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
