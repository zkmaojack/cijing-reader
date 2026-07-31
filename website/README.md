# Jack Mao 独立作品网站

个人软件作品入口，收录语界精读（Yujie Reader）与 FlowLab 2D。首页提供两个项目入口；`/cijing/` 与 `/airflow/` 分别展示项目详情和官方下载方式。

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
GitHub Pages 托管。桌面软件下载文件来自两个项目各自的 GitHub Releases；
语界精读版本信息集中在 `app/home-client.tsx` 顶部的 `RELEASE` 配置中。
