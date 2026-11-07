// Tests for scripts/render-quickstart.mjs (PLAN-0692 T4.2).
//
//   node --test scripts/tests/render-quickstart.test.mjs
//
// The load-bearing assertion is the last group: rendering into a temp stage must
// leave the repository tree untouched (`git status` gains no entry), i.e. the
// renderer can never be used to write docs/QUICKSTART.md into the repo.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import {
  PLACEHOLDER,
  REPO_ROOT,
  TEMPLATE_PATH,
  VERSION_PATTERN,
  assertRendered,
  isInsideTree,
  normalizeText,
  renderQuickstart,
} from "../render-quickstart.mjs";

const SCRIPT_PATH = path.join(REPO_ROOT, "scripts", "render-quickstart.mjs");
const TEMPLATE = fs.readFileSync(TEMPLATE_PATH, "utf8");
const VERSION = "0.1.1";

function runCli(args) {
  const result = spawnSync(process.execPath, [SCRIPT_PATH, ...args], {
    cwd: REPO_ROOT,
    encoding: "utf8",
  });
  return { status: result.status, stdout: result.stdout ?? "", stderr: result.stderr ?? "" };
}

function gitStatus() {
  const result = spawnSync("git", ["-C", REPO_ROOT, "status", "--porcelain", "--untracked-files=all"], {
    encoding: "utf8",
  });
  if (result.status !== 0) return null;
  return result.stdout.split("\n").filter((line) => line.length > 0);
}

function tempDir(t) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "xs-render-quickstart-"));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  return dir;
}

test("template only uses the {{VERSION}} placeholder and carries no machine-specific path", () => {
  const placeholders = [...TEMPLATE.matchAll(/\{\{[^}]*\}\}/g)].map((match) => match[0]);
  assert.ok(placeholders.length > 0, "template must contain at least one placeholder");
  assert.deepEqual([...new Set(placeholders)], [PLACEHOLDER]);
  assert.equal(isInsideTree(TEMPLATE_PATH, REPO_ROOT), true, "template lives in the repo tree");
  assert.doesNotMatch(TEMPLATE, /[A-Za-z]:\\/, "no Windows drive-letter path");
  assert.doesNotMatch(TEMPLATE, /\/(home|Users|root|mnt)\//, "no machine-specific Unix path");
  assert.ok(TEMPLATE.endsWith("\n"), "template ends with a newline");
});

test("render substitutes every placeholder and keeps the literal version line", () => {
  const rendered = renderQuickstart(TEMPLATE, VERSION);
  assert.ok(!rendered.includes("{{"), "no placeholder residue");
  assert.ok(!TEMPLATE.includes(VERSION), "template must not hard-code the version");
  const lines = rendered.split("\n").map((line) => line.trim());
  assert.ok(lines.includes(`xs ${VERSION}`), "literal `xs <version>` line present");
  assert.ok(rendered.includes(VERSION));
});

test("rendered QUICKSTART is UTF-8 with LF endings, no BOM, single trailing newline", () => {
  const rendered = renderQuickstart(TEMPLATE, VERSION);
  assert.ok(!rendered.startsWith("\uFEFF"), "no BOM");
  assert.ok(!rendered.includes("\r"), "LF only");
  assert.ok(rendered.endsWith("\n") && !rendered.endsWith("\n\n"), "exactly one trailing newline");
  const roundTrip = Buffer.from(rendered, "utf8").toString("utf8");
  assert.equal(roundTrip, rendered, "UTF-8 round-trips");
});

test("rendered QUICKSTART covers the spec §2 command surface", () => {
  const rendered = renderQuickstart(TEMPLATE, VERSION);
  for (const needle of [
    "xs --version",
    "xs roots add",
    "xs roots list",
    "xs scan --root",
    "xs scan --root 1",
    "xs list",
    "xs index update --root",
    "xs files",
    "xs search",
    "xs grep",
    "xs read",
    "xs db path",
    "SHA256SUMS",
    "%LOCALAPPDATA%",
    "XDG_DATA_HOME",
    "XINGSHU_DB",
    "XINGSHU_ENV=dev",
  ]) {
    assert.ok(rendered.includes(needle), `QUICKSTART must document: ${needle}`);
  }
  const lines = rendered.split("\n");
  const first = lines.findIndex((line) => line.trim().length > 0);
  assert.match(lines[first], /^# /, "first non-empty line is the title");
  const intro = lines.slice(first + 1, first + 5).join("\n");
  assert.match(intro, /git/i, "Git requirement stated within 3 lines after the title");
});

test("assertRendered rejects placeholder residue", () => {
  assert.throws(
    () => assertRendered("xs 0.1.1\nleft {{VERSION}} behind\n", "0.1.1"),
    /still contains "\{\{"/,
  );
  assert.throws(
    () => assertRendered("xs 0.1.1\n{{OTHER}}\n", "0.1.1"),
    /still contains "\{\{"/,
  );
});

test("assertRendered rejects output without the literal `xs <version>` line", () => {
  assert.throws(() => assertRendered("Xingshu 0.1.1\n", "0.1.1"), /literal line "xs 0\.1\.1"/);
  assert.throws(() => assertRendered("xs 0.1.0\n", "0.1.1"), /literal line "xs 0\.1\.1"/);
});

test("renderQuickstart rejects versions the workflow would never pass", () => {
  for (const bad of ["", "0.1", "v0.1.1", "0.1.1 ", "0.1.1; rm -rf /", "01.1.1.1"]) {
    assert.equal(VERSION_PATTERN.test(bad), false, `expected ${JSON.stringify(bad)} to be rejected`);
    assert.throws(() => renderQuickstart(TEMPLATE, bad), /invalid --version/);
  }
  assert.equal(VERSION_PATTERN.test("0.1.1-rc.1"), true);
  assert.equal(VERSION_PATTERN.test("0.1.1+build.7"), true);
});

test("normalizeText strips BOM, converts CRLF/CR to LF, ensures one trailing newline", () => {
  assert.equal(normalizeText("\uFEFFa\r\nb\rc"), "a\nb\nc\n");
  assert.equal(normalizeText("a\n"), "a\n");
});

test("isInsideTree only treats real descendants of the repo as inside", () => {
  assert.equal(isInsideTree(path.join(REPO_ROOT, "docs", "x.md"), REPO_ROOT), true);
  assert.equal(isInsideTree(REPO_ROOT, REPO_ROOT), true);
  assert.equal(isInsideTree(path.join(REPO_ROOT, "..", "sibling.md"), REPO_ROOT), false);
  assert.equal(isInsideTree(path.join(os.tmpdir(), "QUICKSTART.md"), REPO_ROOT), false);
});

test("CLI renders into a temp stage and leaves the repository tree unchanged", (t) => {
  const before = gitStatus();
  if (before === null) {
    t.skip("git unavailable; the tree-unchanged assertion needs a git worktree");
    return;
  }
  const dir = tempDir(t);
  const out = path.join(dir, "QUICKSTART.md");
  const result = runCli(["--version", VERSION, "--out", out]);

  assert.equal(result.status, 0, `cli exit ${result.status}: ${result.stderr}`);
  assert.match(result.stdout, new RegExp(`quickstart_version=${VERSION.replaceAll(".", "\\.")}`));
  const written = fs.readFileSync(out, "utf8");
  assert.equal(written, renderQuickstart(TEMPLATE, VERSION));

  const after = gitStatus();
  const beforeSet = new Set(before);
  const newEntries = after.filter((line) => !beforeSet.has(line));
  assert.deepEqual(newEntries, [], `rendering must not change the repo tree; new status lines: ${newEntries.join(" | ")}`);

  const renderedInTree = ["QUICKSTART.md", path.join("docs", "release", "QUICKSTART.md")]
    .map((relative) => path.join(REPO_ROOT, relative))
    .filter((candidate) => fs.existsSync(candidate));
  assert.deepEqual(renderedInTree, [], "no rendered QUICKSTART.md may appear inside the repo");
});

test("CLI refuses to write anywhere inside the repository tree", (t) => {
  const target = path.join(REPO_ROOT, "docs", "release", "QUICKSTART.md");
  const existedBefore = fs.existsSync(target);
  const result = runCli(["--version", VERSION, "--out", target]);
  assert.equal(result.status, 2, `expected usage error, got ${result.status}: ${result.stderr}`);
  assert.match(result.stderr, /refusing to write inside the repository tree/);
  assert.equal(fs.existsSync(target), existedBefore, "must not create or overwrite a repo file");
  t.diagnostic(`guarded target: ${path.relative(REPO_ROOT, target)}`);
});

test("CLI reports usage errors with exit 2", () => {
  const missingOut = runCli(["--version", VERSION]);
  assert.equal(missingOut.status, 2);
  assert.match(missingOut.stderr, /--version and --out are required/);

  const missingValue = runCli(["--version", "--out", path.join(os.tmpdir(), "x.md")]);
  assert.equal(missingValue.status, 2);
  assert.match(missingValue.stderr, /--version requires a value/);

  const unknown = runCli(["--version", VERSION, "--out", path.join(os.tmpdir(), "x.md"), "--extra"]);
  assert.equal(unknown.status, 2);
  assert.match(unknown.stderr, /unknown argument "--extra"/);

  const help = runCli(["--help"]);
  assert.equal(help.status, 0);
  assert.match(help.stdout, /--version <V> --out <QUICKSTART\.md>/);
});

test("CLI fails closed when the version is invalid", (t) => {
  const dir = tempDir(t);
  const out = path.join(dir, "QUICKSTART.md");
  const result = runCli(["--version", "not-a-version", "--out", out]);
  assert.equal(result.status, 1);
  assert.match(result.stderr, /invalid --version/);
  assert.equal(fs.existsSync(out), false, "nothing is written on validation failure");
});
