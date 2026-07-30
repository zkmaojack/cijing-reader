import assert from "node:assert/strict";
import { access, readFile } from "node:fs/promises";
import test from "node:test";

const output = new URL("../dist/client/", import.meta.url);

test("exports a GitHub Pages-ready homepage", async () => {
  const html = await readFile(new URL("index.html", output), "utf8");

  assert.match(html, /<title>语界精读/);
  assert.match(html, /\/cijing-reader\/assets\//);
  assert.match(html, /\/cijing-reader\/yujie-logo\.png/);
  assert.match(html, /yujie-reader-v1\.4\.1-portable\.zip/);
  assert.doesNotMatch(html, /(?:src|href)="\/(?!cijing-reader)/);
  await access(new URL(".nojekyll", output));
});
