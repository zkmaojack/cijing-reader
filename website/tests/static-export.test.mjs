import assert from "node:assert/strict";
import { access, readFile } from "node:fs/promises";
import test from "node:test";

const output = new URL("../dist/client/", import.meta.url);
const basePath = (process.env.NEXT_PUBLIC_BASE_PATH ?? "/cijing-reader").replace(
  /\/$/,
  "",
);

test("exports a GitHub Pages-ready portfolio and project routes", async () => {
  const [home, cijing, airflow] = await Promise.all([
    readFile(new URL("index.html", output), "utf8"),
    readFile(new URL("cijing/index.html", output), "utf8"),
    readFile(new URL("airflow/index.html", output), "utf8"),
  ]);

  assert.match(home, /<title>Jack Mao/);
  assert.ok(home.includes(`href="${basePath}/cijing/`));
  assert.ok(home.includes(`href="${basePath}/airflow/`));
  assert.ok(home.includes(`${basePath}/flowlab-preview.png`));

  assert.match(cijing, /<title>语界精读/);
  assert.ok(cijing.includes(`${basePath}/yujie-logo.png`));
  assert.match(cijing, /yujie-reader-v1\.4\.1-portable\.zip/);

  assert.match(airflow, /<title>FlowLab 2D/);
  assert.match(airflow, /FlowLab-2D-Setup-x64\.exe/);
  assert.ok(airflow.includes(`${basePath}/flowlab-preview.png`));

  for (const html of [home, cijing, airflow]) {
    assert.ok(html.includes(`${basePath}/assets/`));
    if (basePath) {
      assert.doesNotMatch(html, /(?:src|href)="\/(?!cijing-reader)/);
    } else {
      assert.doesNotMatch(html, /(?:src|href)="\/cijing-reader\//);
    }
  }
  await access(new URL(".nojekyll", output));
});
