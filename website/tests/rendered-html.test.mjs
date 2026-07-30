import assert from "node:assert/strict";
import { access, readFile } from "node:fs/promises";
import test from "node:test";

const root = new URL("../", import.meta.url);

async function render() {
  const workerUrl = new URL("../dist/server/index.js", import.meta.url);
  workerUrl.searchParams.set("test", `${process.pid}-${Date.now()}`);
  const { default: worker } = await import(workerUrl.href);

  return worker.fetch(
    new Request("http://localhost/", {
      headers: { accept: "text/html", host: "localhost" },
    }),
    {
      ASSETS: {
        fetch: async () => new Response("Not found", { status: 404 }),
      },
    },
    {
      waitUntil() {},
      passThroughOnException() {},
    },
  );
}

test("server-renders the Yujie Reader landing page", async () => {
  const response = await render();
  assert.equal(response.status, 200);
  assert.match(response.headers.get("content-type") ?? "", /^text\/html\b/i);

  const html = await response.text();
  assert.match(html, /<title>语界精读/);
  assert.match(html, /把一篇英文文章/);
  assert.match(html, /下载便携版/);
  assert.match(html, /yujie-reader-v1\.4\.1-portable\.zip/);
  assert.match(html, /Windows 10/);
  assert.doesNotMatch(html, /codex-preview|Your site is taking shape|react-loading-skeleton/i);
});

test("ships product metadata and removes the temporary preview", async () => {
  const [page, homeClient, layout, css, packageJson, hosting] = await Promise.all([
    readFile(new URL("../app/page.tsx", import.meta.url), "utf8"),
    readFile(new URL("../app/home-client.tsx", import.meta.url), "utf8"),
    readFile(new URL("../app/layout.tsx", import.meta.url), "utf8"),
    readFile(new URL("../app/globals.css", import.meta.url), "utf8"),
    readFile(new URL("../package.json", import.meta.url), "utf8"),
    readFile(new URL("../.openai/hosting.json", import.meta.url), "utf8"),
  ]);

  assert.match(page, /force-static/);
  assert.match(homeClient, /YUJIE READER/);
  assert.match(css, /prefers-reduced-motion/);
  assert.match(layout, /og\.png/);
  assert.match(layout, /语界精读/);
  assert.doesNotMatch(packageJson, /react-loading-skeleton/);
  assert.match(hosting, /appgprj_6a6b1613a88481919474df318afc799d/);
  await assert.rejects(access(new URL("../app/_sites-preview", root)));
});
