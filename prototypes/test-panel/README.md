# 右侧测试栏布局原型

运行：`python3 -m http.server 4318 --bind 127.0.0.1 --directory prototypes/test-panel`，打开 http://127.0.0.1:4318/ 。此原型用于原生应用结构验证，结果为明确标注的演示，源码不执行。

参考 CP Editor 官方界面：https://cpeditor.org/img/banner/home-center-2.jpg 。上方运行信息、下方测试数据独立分隔；输入／实际输出／预期输出并排。可拖动两条分隔线，键盘聚焦后可用方向键调整，文本框正常 Enter 换行。控件 hover 只改变文字或细线，不使用阴影和灰色背景块。

已验证：Enter 输入多行；载入500例时DOM仅有当前4行、12个文本框；行数随视口变化。Impeccable layout 扫描未报告检测项，不能替代用户体验评价。原生复现见 `crates/altaed/src/app/test_editors.rs`、`crates/altaed/src/app/workbench.rs` 与 `crates/altaed/src/text_field.rs`。
