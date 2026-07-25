# 词境精读

词境精读是一款面向英语学习者的 Windows 精读工具。粘贴英文文章后，软件会根据学生年级与词汇规则标注单词，并生成适合阅读、打印和分享的排版预览。

## 功能

- 按学生年级识别和标注生词
- 显示音标与中文释义
- 支持自定义词表和标注规则
- 内置离线词典，可在无网络环境下使用
- 可选接入 OpenAI 兼容接口或在线词典
- 实时预览并导出 PDF
- 支持浅色与深色主题

## 下载

Windows 用户可前往 [Releases](https://github.com/zkmaojack/cijing-reader/releases/latest) 下载最新的 EXE 文件。

软件为便携版，无需安装。双击 EXE 后会自动在浏览器中打开操作界面。

> 导出 PDF 需要电脑上已安装 Microsoft Edge 或 Google Chrome。

## 从源码构建

请先安装最新稳定版 Rust，然后在项目目录运行：

```powershell
cargo build --release
```

构建产物位于：

```text
target/release/rust-cijing-reader.exe
```

## 使用说明

1. 双击运行 EXE。
2. 粘贴需要精读的英文文章。
3. 选择学生年级并调整标注规则。
4. 根据需要补充自定义词汇，或启用 AI/网络增强。
5. 在右侧查看预览，并下载 PDF。

API 密钥只用于当次网络请求。请仅使用自己信任的接口地址。

## 反馈

如果遇到问题或有功能建议，欢迎在仓库的 Issues 页面提交反馈。
