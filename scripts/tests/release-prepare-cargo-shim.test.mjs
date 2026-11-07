import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import {
    chmodSync,
    copyFileSync,
    existsSync,
    linkSync,
    mkdirSync,
    mkdtempSync,
    readFileSync,
    readdirSync,
    rmSync,
    writeFileSync,
} from "node:fs";
import os from "node:os";
import path from "node:path";
import test, { after } from "node:test";

import { prepareRelease } from "../release-prepare.mjs";

// `node --test` runs test files as parallel processes that share one temp root, and
// `runRecordDirectories()` below claims every `xs-release-prepare-*` directory that
// appears in `os.tmpdir()` during a `runWithShim` window before deleting it. Redirect
// the temp root for this file only, so the claim window can never cover a run record
// that belongs to another process still in flight.
//
// All three variables are set because `os.tmpdir()` picks them per platform and the
// precedence differs: POSIX reads TMPDIR, then TMP, then TEMP, while Windows (measured
// on v24.19.0) ignores TMPDIR and uses TEMP, then TMP, then `%SystemRoot%\temp`. A single
// variable would leave the other platform pointing at the shared root.
const TEMP_ROOT = mkdtempSync(path.join(os.tmpdir(), "xs-cargo-shim-temp-"));
process.env.TMPDIR = TEMP_ROOT;
process.env.TEMP = TEMP_ROOT;
process.env.TMP = TEMP_ROOT;
after(() => rmSync(TEMP_ROOT, { recursive: true, force: true }));

// Real-process tests: a fake `cargo` sits on PATH and a throwaway workspace is the cwd,
// so the wrapper's spawn layer (PATH lookup, argv, exit code, run record) is exercised
// instead of a stubbed `run` function.
//
// Windows cannot spawn a bare PATH script (libuv needs a real executable), so the fake
// binary is a hardlink/copy of the running node executable. Node then treats the first
// argument as a script path, which is why the fake workspace ships extension-less
// dispatchers named `release` and `metadata`.

const PACKAGE_NAMES = ["xingshu-cli", "xingshu-core", "xingshu-server"];

const SHIM_BODY = `
const fs = require("node:fs");
const path = require("node:path");

module.exports = (kind) => {
    const args = process.argv.slice(2);
    const mode = process.env.XS_SHIM_MODE || "bump";
    const cwd = process.cwd();
    const manifest = path.join(cwd, "Cargo.toml");
    if (process.env.XS_SHIM_LOG) {
        fs.appendFileSync(process.env.XS_SHIM_LOG, JSON.stringify({ kind, args, mode }) + "\\n");
    }
    const readVersion = () => /version = "(\\d+\\.\\d+\\.\\d+)"/.exec(fs.readFileSync(manifest, "utf8"))[1];

    if (kind === "release") {
        if (args.includes("--version")) {
            process.stdout.write("cargo-release " + (process.env.XS_SHIM_VERSION || "1.1.7") + "\\n");
            return;
        }
        if (mode === "bump" && args.includes("--execute")) {
            const version = readVersion();
            const [major, minor, patch] = version.split(".").map(Number);
            const next = major + "." + minor + "." + (patch + 1);
            fs.writeFileSync(
                manifest,
                fs.readFileSync(manifest, "utf8").replace('version = "' + version + '"', 'version = "' + next + '"'),
            );
            process.stdout.write("Upgrading workspace to version " + next + "\\n");
        }
        // "noop" mode exits 0 without touching the manifest: the historical false green.
        return;
    }

    if (kind === "metadata") {
        const version = readVersion();
        const packages = ${JSON.stringify(PACKAGE_NAMES)}.map((name) => ({
            id: "path+file:///" + name + "#" + name + "@" + version,
            name,
            version,
        }));
        process.stdout.write(JSON.stringify({ packages, workspace_members: packages.map((pkg) => pkg.id) }));
        return;
    }

    process.stderr.write("unexpected fake cargo invocation: " + kind + " " + args.join(" ") + "\\n");
    process.exitCode = 2;
};
`;

function git(cwd, args) {
    const result = spawnSync(
        "git",
        ["-c", "user.email=shim@example.com", "-c", "user.name=shim", "-c", "commit.gpgsign=false", ...args],
        { cwd, encoding: "utf8" },
    );
    assert.equal(result.status, 0, `git ${args.join(" ")} failed: ${result.stderr}`);
    return result.stdout;
}

function createSandbox(version = "0.1.0") {
    const root = mkdtempSync(path.join(os.tmpdir(), "xs-cargo-shim-"));
    const shimDirectory = path.join(root, "bin");
    const workspace = path.join(root, "ws");
    const logPath = path.join(root, "cargo-argv.log");
    mkdirSync(shimDirectory);
    mkdirSync(workspace);

    const shimPath = path.join(shimDirectory, process.platform === "win32" ? "cargo.exe" : "cargo");
    try {
        linkSync(process.execPath, shimPath);
    } catch {
        copyFileSync(process.execPath, shimPath);
    }
    if (process.platform !== "win32") chmodSync(shimPath, 0o755);

    writeFileSync(
        path.join(workspace, "Cargo.toml"),
        '[workspace]\nresolver = "2"\n\n[workspace.package]\nedition = "2024"\nversion = "' + version + '"\n',
    );
    writeFileSync(path.join(workspace, "xs-cargo-shim.cjs"), SHIM_BODY);
    for (const kind of ["release", "metadata"]) {
        writeFileSync(path.join(workspace, kind), `require("./xs-cargo-shim.cjs")(${JSON.stringify(kind)});\n`);
    }
    git(workspace, ["init", "-q"]);
    git(workspace, ["add", "-A"]);
    git(workspace, ["commit", "-q", "-m", "shim workspace"]);

    return { root, shimDirectory, workspace, logPath };
}

function readWorkspaceVersion(workspace) {
    return /version = "(\d+\.\d+\.\d+)"/.exec(readFileSync(path.join(workspace, "Cargo.toml"), "utf8"))[1];
}

function cargoCalls(logPath) {
    if (!existsSync(logPath)) return [];
    return readFileSync(logPath, "utf8")
        .split("\n")
        .filter(Boolean)
        .map((line) => JSON.parse(line));
}

function versionStepCalls(logPath) {
    return cargoCalls(logPath).filter((call) => call.kind === "release" && !call.args.includes("--version"));
}

function runRecordDirectories() {
    return new Set(readdirSync(os.tmpdir()).filter((name) => name.startsWith("xs-release-prepare-")));
}

async function runWithShim({ sandbox, argv, mode }) {
    const before = runRecordDirectories();
    const saved = {
        PATH: process.env.PATH,
        XS_SHIM_MODE: process.env.XS_SHIM_MODE,
        XS_SHIM_LOG: process.env.XS_SHIM_LOG,
    };
    process.env.PATH = sandbox.shimDirectory + path.delimiter + saved.PATH;
    process.env.XS_SHIM_MODE = mode;
    process.env.XS_SHIM_LOG = sandbox.logPath;
    let error;
    let result;
    try {
        result = await prepareRelease({ argv, cwd: sandbox.workspace, log: () => {} });
    } catch (caught) {
        error = caught;
    } finally {
        for (const [key, value] of Object.entries(saved)) {
            if (value === undefined) delete process.env[key];
            else process.env[key] = value;
        }
    }
    const createdRecords = [...runRecordDirectories()].filter((name) => !before.has(name));
    return { result, error, createdRecords };
}

function readRecords(records) {
    assert.equal(records.length >= 1, true, "expected preserved run record directories");
    return records
        .map((name) => path.join(os.tmpdir(), name, "run.json"))
        .filter((runRecordPath) => existsSync(runRecordPath))
        .map((runRecordPath) => ({ path: runRecordPath, record: JSON.parse(readFileSync(runRecordPath, "utf8")) }));
}

function versionStepRecord(records) {
    const matches = readRecords(records).filter(
        ({ record }) =>
            record.command?.[0] === "cargo" &&
            record.command?.[1] === "release" &&
            record.command?.[2] === "version",
    );
    assert.equal(matches.length, 1, "expected exactly one cargo release version-step run record");
    return matches[0];
}

function cleanup(sandbox, records) {
    for (const name of records) rmSync(path.join(os.tmpdir(), name), { recursive: true, force: true });
    rmSync(sandbox.root, { recursive: true, force: true });
}

test("execute passes --no-confirm so cargo-release cannot silently decline", async () => {
    const sandbox = createSandbox("0.1.0");
    let records = [];
    try {
        const outcome = await runWithShim({ sandbox, argv: ["--bump", "patch", "--execute"], mode: "bump" });
        records = outcome.createdRecords;
        assert.equal(outcome.error, undefined, `unexpected failure: ${outcome.error?.message}`);

        const steps = versionStepCalls(sandbox.logPath);
        assert.equal(steps.length, 1);
        assert.deepEqual(steps[0].args, ["version", "patch", "--workspace", "--execute", "--no-confirm"]);
        for (const forbidden of ["publish", "tag", "push"]) {
            assert.equal(steps[0].args.includes(forbidden), false, `execute must not pass ${forbidden}`);
        }
        assert.equal(readWorkspaceVersion(sandbox.workspace), "0.1.1");
    } finally {
        cleanup(sandbox, records);
    }
});

test("dry-run keeps the preview semantics and never passes --no-confirm", async () => {
    const sandbox = createSandbox("0.1.0");
    let records = [];
    try {
        const outcome = await runWithShim({ sandbox, argv: ["--bump", "patch"], mode: "bump" });
        records = outcome.createdRecords;
        assert.equal(outcome.error, undefined, `unexpected failure: ${outcome.error?.message}`);

        const steps = versionStepCalls(sandbox.logPath);
        assert.equal(steps.length, 1);
        assert.deepEqual(steps[0].args, ["version", "patch", "--workspace"]);
        assert.equal(steps[0].args.includes("--no-confirm"), false);
        assert.equal(steps[0].args.includes("--execute"), false);
        assert.equal(readWorkspaceVersion(sandbox.workspace), "0.1.0");
    } finally {
        cleanup(sandbox, records);
    }
});

test("exit 0 without a version change is rejected instead of reported as success", async () => {
    const sandbox = createSandbox("0.1.0");
    let records = [];
    try {
        const outcome = await runWithShim({ sandbox, argv: ["--bump", "patch", "--execute"], mode: "noop" });
        records = outcome.createdRecords;

        assert.ok(outcome.error, "a silent no-op must fail the wrapper");
        assert.match(outcome.error.message, /exited 0 but the workspace version is still 0\.1\.0/);
        assert.match(outcome.error.message, /refusing to report success/);
        assert.equal(outcome.error.terminalStatus, "failed");
        assert.equal(readWorkspaceVersion(sandbox.workspace), "0.1.0");

        const { record } = versionStepRecord(records);
        assert.equal(record.terminalStatus, "failed");
        assert.equal(record.outcome, "no-op");
        assert.match(record.detail, /exited 0 but the workspace version is still 0\.1\.0/);
    } finally {
        cleanup(sandbox, records);
    }
});

test("a real version change is reported as completed and keeps its run record", async () => {
    const sandbox = createSandbox("0.1.0");
    let records = [];
    try {
        const outcome = await runWithShim({ sandbox, argv: ["--bump", "patch", "--execute"], mode: "bump" });
        records = outcome.createdRecords;
        assert.equal(outcome.error, undefined, `unexpected failure: ${outcome.error?.message}`);
        assert.equal(outcome.result.action, "cargo-release-version-step");

        const { path: runRecordPath, record } = versionStepRecord(records);
        assert.equal(record.terminalStatus, "completed");
        assert.equal(record.outcome, undefined);
        assert.equal(record.exitCode, 0);
        assert.deepEqual(record.command, [
            "cargo",
            "release",
            "version",
            "patch",
            "--workspace",
            "--execute",
            "--no-confirm",
        ]);
        assert.equal(existsSync(path.dirname(runRecordPath)), true);
        assert.equal(existsSync(path.join(path.dirname(runRecordPath), "stdout.log")), true);
    } finally {
        cleanup(sandbox, records);
    }
});
