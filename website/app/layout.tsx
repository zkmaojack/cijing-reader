import type { Metadata } from "next";
import "./globals.css";

export const dynamic = "force-static";

const basePath = process.env.NEXT_PUBLIC_BASE_PATH ?? "";
const siteOrigin =
  process.env.NEXT_PUBLIC_SITE_ORIGIN ??
  "https://yujie-reader.jackmao.chatgpt.site";
const canonicalUrl = `${siteOrigin}${basePath}/`;
const publicAsset = (pathname: string) =>
  `${siteOrigin}${basePath}${pathname}`;

export const metadata: Metadata = {
  metadataBase: new URL(canonicalUrl),
  title: {
    default: "Jack Mao · 独立作品｜语界精读 × FlowLab 2D",
    template: "%s · Jack Mao",
  },
  description:
    "Jack Mao 的独立软件作品集：语界精读英语学习工具与 FlowLab 2D 实时二维流体仿真工作台。",
  applicationName: "Jack Mao · Selected Works",
  keywords: [
    "Jack Mao",
    "独立开发",
    "语界精读",
    "Yujie Reader",
    "FlowLab 2D",
    "二维流体仿真",
  ],
  authors: [{ name: "Jack Mao", url: "https://github.com/zkmaojack" }],
  alternates: {
    canonical: canonicalUrl,
  },
  icons: {
    icon: `${basePath}/favicon.ico`,
    shortcut: `${basePath}/favicon.ico`,
    apple: `${basePath}/yujie-logo.png`,
  },
  openGraph: {
    type: "website",
    locale: "zh_CN",
    alternateLocale: "en_US",
    url: canonicalUrl,
    siteName: "Jack Mao · Selected Works",
    title: "Jack Mao · 独立作品",
    description:
      "一件帮助人读懂语言，一件帮助人看见流动。探索语界精读与 FlowLab 2D。",
    images: [
      {
        url: publicAsset("/og.png"),
        width: 1734,
        height: 907,
        alt: "Jack Mao 独立作品：Cijing Reader 与 FlowLab 2D",
      },
    ],
  },
  twitter: {
    card: "summary_large_image",
    title: "Jack Mao · Selected Works",
    description: "Cijing Reader × FlowLab 2D",
    images: [publicAsset("/og.png")],
  },
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="zh-CN">
      <body>{children}</body>
    </html>
  );
}
