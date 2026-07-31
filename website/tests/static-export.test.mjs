import assert from "node:assert/strict";
import { access, readFile } from "node:fs/promises";
import test from "node:test";

const output = new URL("../dist/client/", import.meta.url);

test("exports a GitHub Pages-ready portfolio and project routes", async () => {
  const [home, cijing, airflow] = await Promise.all([
    readFile(new URL("index.html", output), "utf8"),
    readFile(new URL("cijing/index.html", output), "utf8"),
    readFile(new URL("airflow/index.html", output), "utf8"),
  ]);

  assert.match(home, /<title>Jack Mao/);
  assert.match(home, /href="\/cijing-reader\/cijing\//);
  assert.match(home, /href="\/cijing-reader\/airflow\//);
  assert.match(home, /\/cijing-reader\/flowlab-preview\.png/);

  assert.match(cijing, /<title>语界精读/);
  assert.match(cijing, /\/cijing-reader\/yujie-logo\.png/);
  assert.match(cijing, /yujie-reader-v1\.4\.1-portable\.zip/);

  assert.match(airflow, /<title>FlowLab 2D/);
  assert.match(airflow, /FlowLab-2D-Setup-x64\.exe/);
  assert.match(airflow, /\/cijing-reader\/flowlab-preview\.png/);

  for (const html of [home, cijing, airflow]) {
    assert.match(html, /\/cijing-reader\/assets\//);
    assert.doesNotMatch(html, /(?:src|href)="\/(?!cijing-reader)/);
  }
  await access(new URL(".nojekyll", output));
});
