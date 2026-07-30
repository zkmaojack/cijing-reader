# 语界精读官网

语界精读（Yujie Reader）的产品官网与官方下载入口。

## 本地开发

```powershell
pnpm install
pnpm dev
```

生产构建：

```powershell
pnpm build
```

GitHub Pages 静态构建：

```powershell
$env:GITHUB_PAGES = "true"
pnpm build
```

静态产物输出到 `dist/client`。网站使用 vinext 构建，可由 Sites 或
GitHub Pages 托管。桌面软件下载文件来自项目的 GitHub Releases；版本
信息集中在 `app/page.tsx` 顶部的 `RELEASE` 配置中。
