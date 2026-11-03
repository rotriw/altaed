# 界面资源

`status-success.svg` 是本项目绘制的圆圈勾选图标。`welcome-wordmark.svg` 由脚本 `scripts/generate_wordmark.swift` 使用已固定的 Zed 基线内 IBM Plex Sans SemiBold 字形轮廓生成，仅包含 altaed 六个字母和高斯文字阴影。字体来源及 OFL 许可证见 `本目录 LICENSE-IBM-PLEX.txt 以及 vendor/zed/assets/fonts/ibm-plex-sans/license.txt`。无需运行时读取开发机字体路径，网页与原生共用轮廓资源。修改字形后同步生成的 metrics 到 welcome-layout.json 的 wordmark 字段。

背景 ALTAED 为普通文本排版，无圆环、渐变或远程素材。风格参考用户指定的 [ENDFIELD 官方界面](https://endfield.hypergryph.com/)，没有使用其商标图形或游戏素材。
