import assert from "node:assert/strict";
import { access, readFile } from "node:fs/promises";
import test from "node:test";

const root = new URL("../", import.meta.url);

async function render(pathname = "/") {
  const workerUrl = new URL("../dist/server/index.js", import.meta.url);
  workerUrl.searchParams.set("test", `${process.pid}-${Date.now()}-${pathname}`);
  const { default: worker } = await import(workerUrl.href);

  return worker.fetch(
    new Request(new URL(pathname, "http://localhost/"), {
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

test("server-renders the personal portfolio homepage", async () => {
  const response = await render();
  assert.equal(response.status, 200);
  assert.match(response.headers.get("content-type") ?? "", /^text\/html\b/i);

  const html = await response.text();
  assert.match(html, /<title>Jack Mao/);
  assert.match(html, /选择一个入口/);
  assert.match(html, /进入语界精读/);
  assert.match(html, /进入 FlowLab 2D/);
  assert.match(html, /yujie-reader-v1\.4\.1-portable\.zip/);
  assert.match(html, /FlowLab-2D-Setup-x64\.exe/);
  assert.doesNotMatch(html, /codex-preview|Your site is taking shape|react-loading-skeleton/i);
});

test("server-renders both project detail routes", async () => {
  const [cijingResponse, airflowResponse] = await Promise.all([
    render("/cijing"),
    render("/airflow"),
  ]);

  assert.equal(cijingResponse.status, 200);
  assert.equal(airflowResponse.status, 200);

  const [cijingHtml, airflowHtml] = await Promise.all([
    cijingResponse.text(),
    airflowResponse.text(),
  ]);

  assert.match(cijingHtml, /<title>语界精读/);
  assert.match(cijingHtml, /把一篇英文文章/);
  assert.match(cijingHtml, /下载便携版/);
  assert.match(cijingHtml, /Windows 10/);
  assert.match(airflowHtml, /<title>FlowLab 2D/);
  assert.match(airflowHtml, /实时二维流体实验台/);
  assert.match(airflowHtml, /FlowLab-2D-Setup-x64\.exe/);
  assert.match(airflowHtml, /教学和概念比较/);
});

test("ships portfolio metadata and removes the temporary preview", async () => {
  const [
    page,
    cijingPage,
    airflowPage,
    homeClient,
    layout,
    css,
    portfolioCss,
    airflowCss,
    packageJson,
    hosting,
  ] = await Promise.all([
    readFile(new URL("../app/page.tsx", import.meta.url), "utf8"),
    readFile(new URL("../app/cijing/page.tsx", import.meta.url), "utf8"),
    readFile(new URL("../app/airflow/page.tsx", import.meta.url), "utf8"),
    readFile(new URL("../app/home-client.tsx", import.meta.url), "utf8"),
    readFile(new URL("../app/layout.tsx", import.meta.url), "utf8"),
    readFile(new URL("../app/globals.css", import.meta.url), "utf8"),
    readFile(new URL("../app/portfolio.module.css", import.meta.url), "utf8"),
    readFile(new URL("../app/airflow/airflow.module.css", import.meta.url), "utf8"),
    readFile(new URL("../package.json", import.meta.url), "utf8"),
    readFile(new URL("../.openai/hosting.json", import.meta.url), "utf8"),
  ]);

  assert.match(page, /force-static/);
  assert.match(page, /Selected works/);
  assert.match(cijingPage, /HomeClient/);
  assert.match(airflowPage, /FlowLab 2D/);
  assert.match(homeClient, /YUJIE READER/);
  assert.match(css, /prefers-reduced-motion/);
  assert.match(portfolioCss, /prefers-reduced-motion/);
  assert.match(airflowCss, /prefers-reduced-motion/);
  assert.match(layout, /og\.png/);
  assert.match(layout, /Jack Mao/);
  assert.doesNotMatch(packageJson, /react-loading-skeleton/);
  assert.match(hosting, /appgprj_6a6b1613a88481919474df318afc799d/);
  await assert.rejects(access(new URL("../app/_sites-preview", root)));
});
