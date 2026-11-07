#!/usr/bin/env node
// Render docs/release/QUICKSTART.template.md into a release candidate stage.
//
// Frozen command surface (PLAN-0692 F4) — identical on Windows and Linux:
//   node scripts/render-quickstart.mjs --version <V> --out <STAGE>/QUICKSTART.md
//
// The repository tree is never a valid output location: --out must point at the
// candidate stage (or any temp dir), and a path inside this repository is
// rejected with exit 2. Node 24+, zero dependencies.

import { createHash } from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { pathToFileURL } from "node:url";

/** Same semver shape the candidate workflow enforces on `xs --version`. */
export const VERSION_PATTERN = /^[0-9]+\.[0-9]+\.[0-9]+([+-][0-9A-Za-z.-]+)?$/;

/** The only placeholder the template may contain. */
export const PLACEHOLDER = "{{VERSION}}";

/** Repository root = parent of this script's directory. */
export const REPO_ROOT = path.resolve(import.meta.dirname, "..");

export const TEMPLATE_PATH = path.join(REPO_ROOT, "docs", "release", "QUICKSTART.template.md");

const USAGE = `Usage: node scripts/render-quickstart.mjs --version <V> --out <QUICKSTART.md>

  --version <V>   version to render into the template (e.g. 0.1.1); must match xs --version
  --out <path>    output file; must be outside the repository tree (candidate stage / temp)
  -h, --help      print this help

Exit codes: 0 = written, 1 = render/validation failure, 2 = usage error.`;

/** Normalize source text: strip BOM, CRLF -> LF, guarantee a single trailing newline. */
export function normalizeText(text) {
  const lf = text.replace(/^\uFEFF/, "").replace(/\r\n/g, "\n").replace(/\r/g, "\n");
  return lf.endsWith("\n") ? lf : `${lf}\n`;
}

/**
 * Validate rendered output: no placeholder residue, and the literal version line
 * `xs <version>` must be present (it has to equal `xs --version` character for
 * character, which is what the candidate smoke test checks against the binary).
 */
export function assertRendered(text, version) {
  const lines = text.split("\n");
  const leftovers = [];
  lines.forEach((line, index) => {
    if (line.includes("{{")) leftovers.push(`line ${index + 1}: ${line.trim()}`);
  });
  if (leftovers.length > 0) {
    throw new Error(`rendered QUICKSTART still contains "{{": ${leftovers.join(" | ")}`);
  }
  const wanted = `xs ${version}`;
  if (!lines.some((line) => line.trim() === wanted)) {
    throw new Error(`rendered QUICKSTART must contain the literal line "${wanted}" (xs --version output)`);
  }
  return text;
}

/** Render a template string for one version. */
export function renderQuickstart(template, version) {
  if (typeof version !== "string" || !VERSION_PATTERN.test(version)) {
    throw new Error(`invalid --version ${JSON.stringify(version)}; expected a semver like 0.1.1`);
  }
  const rendered = normalizeText(template).split(PLACEHOLDER).join(version);
  return assertRendered(rendered, version);
}

/** True when `target` is `root` or lives under it. */
export function isInsideTree(target, root) {
  const relative = path.relative(root, target);
  return relative === "" || (!relative.startsWith(`..${path.sep}`) && relative !== ".." && !path.isAbsolute(relative));
}

function fail(message, code) {
  process.stderr.write(`render-quickstart: ${message}\n`);
  process.exit(code);
}

function parseArgs(argv) {
  const options = { version: undefined, out: undefined, help: false };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "-h" || arg === "--help") {
      options.help = true;
    } else if (arg === "--version" || arg === "--out") {
      const value = argv[index + 1];
      if (value === undefined || value.startsWith("--")) {
        throw new Error(`${arg} requires a value`);
      }
      options[arg === "--version" ? "version" : "out"] = value;
      index += 1;
    } else {
      throw new Error(`unknown argument ${JSON.stringify(arg)}`);
    }
  }
  return options;
}

function main(argv) {
  let options;
  try {
    options = parseArgs(argv);
  } catch (error) {
    process.stderr.write(`render-quickstart: ${error.message}\n\n${USAGE}\n`);
    process.exit(2);
  }
  if (options.help) {
    process.stdout.write(`${USAGE}\n`);
    return;
  }
  if (options.version === undefined || options.out === undefined) {
    process.stderr.write(`render-quickstart: --version and --out are required\n\n${USAGE}\n`);
    process.exit(2);
  }

  const outPath = path.resolve(options.out);
  if (isInsideTree(outPath, REPO_ROOT)) {
    fail(
      `refusing to write inside the repository tree: ${outPath}\n` +
        "  pass --out pointing at the candidate stage, e.g. --out \"$RUNNER_TEMP/xs-candidate-stage/QUICKSTART.md\"",
      2,
    );
  }

  let rendered;
  try {
    const template = fs.readFileSync(TEMPLATE_PATH, "utf8");
    rendered = renderQuickstart(template, options.version);
  } catch (error) {
    fail(`${error.message}\n  template: ${TEMPLATE_PATH}`, 1);
  }

  try {
    fs.mkdirSync(path.dirname(outPath), { recursive: true });
    fs.writeFileSync(outPath, rendered, { encoding: "utf8" });
  } catch (error) {
    fail(`cannot write ${outPath}: ${error.message}`, 1);
  }

  const bytes = Buffer.byteLength(rendered, "utf8");
  const digest = createHash("sha256").update(rendered, "utf8").digest("hex");
  const written = fs.readFileSync(outPath, "utf8");
  if (written !== rendered) {
    fail(`read-back mismatch for ${outPath} (${Buffer.byteLength(written, "utf8")} of ${bytes} bytes)`, 1);
  }

  process.stdout.write(
    [
      `wrote ${outPath} (${bytes} bytes)`,
      `quickstart_version=${options.version}`,
      `quickstart_bytes=${bytes}`,
      `quickstart_sha256=${digest}`,
      `quickstart_version_line=xs ${options.version}`,
    ].join("\n") + "\n",
  );
}

const invokedDirectly = process.argv[1] !== undefined && pathToFileURL(process.argv[1]).href === import.meta.url;
if (invokedDirectly) {
  main(process.argv.slice(2));
}
