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
    title: "语界精读｜把英文文章变成适合学生年级的精读讲义",
    description:
      "Windows 英语精读工具：按年级标出生词，生成释义与注音，添加句段讲解，并导出 DOCX / PDF。",
    applicationName: "语界精读 · Yujie Reader",
    keywords: [
      "英语精读",
      "分级词汇",
      "英语学习",
      "教师讲义",
      "IPA",
      "Yujie Reader",
    ],
    authors: [{ name: "Yujie Reader" }],
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
      siteName: "语界精读 · Yujie Reader",
      title: "语界精读｜把英文文章读成自己的语言地图",
      description:
        "按学生年级标出生词，补充释义、注音和句段讲解，再直接导出精读讲义。",
      images: [
        {
          url: publicAsset("/og.png"),
          width: basePath ? 512 : 1200,
          height: basePath ? 512 : 630,
          alt: "语界精读 · 在语境中，读懂世界",
        },
      ],
    },
    twitter: {
      card: "summary_large_image",
      title: "语界精读｜Yujie Reader",
      description: "把任意英文文章，变成适合学生年级的精读讲义。",
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
