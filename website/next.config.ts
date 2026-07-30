import type { NextConfig } from "next";

const isGitHubPages = process.env.GITHUB_PAGES === "true";
const basePath = isGitHubPages
  ? (process.env.NEXT_PUBLIC_BASE_PATH ?? "/cijing-reader").replace(/\/$/, "")
  : "";

const nextConfig: NextConfig = {
  ...(isGitHubPages
    ? {
        output: "export" as const,
        assetPrefix: basePath,
        trailingSlash: true,
      }
    : {}),
  env: {
    NEXT_PUBLIC_BASE_PATH: basePath,
    NEXT_PUBLIC_SITE_ORIGIN: isGitHubPages
      ? (process.env.NEXT_PUBLIC_SITE_ORIGIN ?? "https://zkmaojack.github.io")
      : "https://yujie-reader.jackmao.chatgpt.site",
  },
};

export default nextConfig;
