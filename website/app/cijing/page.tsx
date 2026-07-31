import type { Metadata } from "next";
import HomeClient from "../home-client";

export const dynamic = "force-static";

const basePath = process.env.NEXT_PUBLIC_BASE_PATH ?? "";
const siteOrigin =
  process.env.NEXT_PUBLIC_SITE_ORIGIN ??
  "https://yujie-reader.jackmao.chatgpt.site";

export const metadata: Metadata = {
  title: "语界精读｜按学习者年级生成英文精读讲义",
  description:
    "按年级标出生词，生成多语言释义与 IPA，添加句段讲解，并导出 DOCX / PDF。",
  alternates: {
    canonical: `${siteOrigin}${basePath}/cijing/`,
  },
};

export default function CijingPage() {
  return <HomeClient />;
}
