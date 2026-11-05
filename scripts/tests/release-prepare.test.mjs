import assert from "node:assert/strict";
import { existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import {
    assertCargoReleaseVersion,
    parseOptions,
    planVersionStep,
    prepareRelease,
    runFileLogged,
    terminateProcessTree,
} from "../release-prepare.mjs";

const PACKAGE_NAMES = ["xingshu-cli", "xingshu-core", "xingshu-server"];

function cargoMetadata(version = "0.1.0") {
    const packages = PACKAGE_NAMES.map((name) => ({
        id: `path+file:///tmp/xingshu/${name}#${name}@${version}`,
        name,
        version,
    }));
    return JSON.stringify({
        packages,
        workspace_members: packages.map((pkg) => pkg.id),
    });
}

function processIsAlive(pid) {
    try {
        process.kill(pid, 0);
        return true;
    } catch (error) {
        if (error?.code === "ESRCH") return false;
        if (error?.code === "EPERM") return true;
        throw error;
    }
}

async function waitForProcessExit(pid, timeoutMs) {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
        if (!processIsAlive(pid)) return true;
        await new Promise((resolve) => setTimeout(resolve, 50));
    }
    return !processIsAlive(pid);
}

test("defaults to a dry-run minor bump", () => {
    const options = parseOptions([], {});
    const plan = planVersionStep("0.1.0", options);

    assert.equal(options.bump, "minor");
    assert.equal(options.bumpExplicit, false);
    assert.equal(options.execute, false);
    assert.deepEqual(plan.args, ["release", "version", "minor", "--workspace"]);
});

test("allows a maintainer to explicitly select patch", () => {
    const options = parseOptions(["--bump", "patch"], {});
    const plan = planVersionStep("0.1.0", options);

    assert.equal(options.bump, "patch");
    assert.equal(options.bumpExplicit, true);
    assert.deepEqual(plan.args, ["release", "version", "patch", "--workspace"]);
});

test("requires the pinned cargo-release tool version for non-first releases", () => {
    assert.equal(assertCargoReleaseVersion("cargo-release 0.25.20"), "0.25.20");
    assert.throws(
        () => assertCargoReleaseVersion("cargo-release 1.1.7"),
        /cargo-release 0\.25\.20 is required/,
    );
    assert.throws(
        () => assertCargoReleaseVersion("cargo release tool missing"),
        /could not determine the installed cargo-release version/,
    );
});

test("passes mise usage values without changing the default", () => {
    const options = parseOptions([], {
        usage_bump: "patch",
        usage_execute: "true",
    });

    assert.equal(options.bump, "patch");
    assert.equal(options.execute, true);
});

test("supports an explicit mise dry-run and rejects conflicting mode flags", () => {
    const options = parseOptions([], { usage_dry_run: "true" });
    assert.equal(options.execute, false);
    assert.equal(options.dryRunExplicit, true);

    assert.throws(
        () => parseOptions(["--execute", "--dry-run"], {}),
        /cannot be used together/,
    );
});

test("first stable release keeps workspace 0.1.0 without running cargo-release", async () => {
    const calls = [];
    const output = [];
    const result = await prepareRelease({
        argv: ["--first-release", "--execute"],
        cwd: "/tmp/xingshu",
        run(command, args) {
            calls.push({ command, args });
            assert.equal(command, "cargo");
            assert.deepEqual(args, [
                "metadata",
                "--no-deps",
                "--locked",
                "--format-version",
                "1",
            ]);
            return cargoMetadata("0.1.0");
        },
        log(message) {
            output.push(message);
        },
    });

    assert.equal(result.action, "keep-first-release-version");
    assert.equal(calls.length, 1);
    assert.match(output.join("\n"), /no version bump or Cargo release command will run/);
});

test("first stable release rejects an unexpected workspace version", () => {
    assert.throws(
        () => planVersionStep("0.1.1", { firstRelease: true, execute: false }),
        /first release requires workspace version 0\.1\.0/,
    );
});

test("first stable release cannot request an unrelated bump", () => {
    assert.throws(
        () => parseOptions(["--first-release", "--bump", "minor"], {}),
        /cannot be combined with an explicit --bump/,
    );
});

test("major bumps are blocked before 1.0 and available under standard SemVer afterward", () => {
    const majorOptions = parseOptions(["--bump", "major"], {});
    assert.throws(
        () => planVersionStep("0.9.0", majorOptions),
        /major bumps are not allowed before 1\.0\.0/,
    );
    assert.deepEqual(planVersionStep("1.0.0", majorOptions).args, [
        "release",
        "version",
        "major",
        "--workspace",
    ]);
    assert.deepEqual(
        planVersionStep("1.0.0", parseOptions([], {})).args,
        ["release", "version", "minor", "--workspace"],
    );
});

test("dry-run never passes cargo-release execute flags or non-version steps", () => {
    const plan = planVersionStep("0.3.4", {
        bump: "minor",
        execute: false,
        firstRelease: false,
    });

    assert.deepEqual(plan.args, ["release", "version", "minor", "--workspace"]);
    assert.equal(plan.args.includes("publish"), false);
    assert.equal(plan.args.includes("tag"), false);
    assert.equal(plan.args.includes("push"), false);
});

test("execute only runs cargo-release workspace version step", async () => {
    const calls = [];
    const result = await prepareRelease({
        argv: ["--bump", "patch", "--execute"],
        cwd: "/tmp/xingshu",
        run(command, args) {
            calls.push({ command, args });
            if (args[0] === "metadata") return cargoMetadata("0.2.0");
            if (args[0] === "release" && args[1] === "--version") return "cargo-release 0.25.20";
            if (args[0] === "status") return "";
            return "version preview";
        },
        log() {},
    });

    assert.equal(result.action, "cargo-release-version-step");
    assert.deepEqual(calls.map((call) => call.args), [
        ["metadata", "--no-deps", "--locked", "--format-version", "1"],
        ["release", "--version"],
        ["status", "--porcelain", "--untracked-files=all"],
        ["release", "version", "patch", "--workspace", "--execute"],
    ]);
});

test("execute refuses to change versions in a dirty worktree", async () => {
    const calls = [];
    await assert.rejects(
        () =>
            prepareRelease({
                argv: ["--execute"],
                cwd: "/tmp/xingshu",
                run(command, args) {
                    calls.push({ command, args });
                    if (args[0] === "metadata") return cargoMetadata("0.2.0");
                    if (args[0] === "release" && args[1] === "--version") return "cargo-release 0.25.20";
                    if (args[0] === "status") return " M CHANGELOG.md";
                    throw new Error("cargo-release version step must not run for a dirty tree");
                },
                log() {},
            }),
        /refusing to change versions in a dirty worktree/,
    );
    assert.equal(calls.some((call) => call.args[1] === "version"), false);
});

test("timeout terminates the child process tree and records kill confirmation", async () => {
    const testDirectory = mkdtempSync(path.join(os.tmpdir(), "xs-release-tree-test-"));
    const childPidPath = path.join(testDirectory, "grandchild.pid");
    const parentScript = [
        'const { spawn } = require("node:child_process");',
        'const fs = require("node:fs");',
        'const child = spawn(process.execPath, ["-e", "setInterval(() => {}, 1000)"], { stdio: "ignore" });',
        'fs.writeFileSync(process.env.XS_RELEASE_TEST_CHILD_PID_FILE, String(child.pid));',
        'setInterval(() => {}, 1000);',
    ].join("\n");
    let timeoutError;
    let grandchildPid;

    try {
        await assert.rejects(
            () =>
                runFileLogged(process.execPath, ["-e", parentScript], {
                    cwd: testDirectory,
                    timeoutMs: 1_500,
                    env: { XS_RELEASE_TEST_CHILD_PID_FILE: childPidPath },
                }),
            (error) => {
                timeoutError = error;
                assert.equal(error.terminalStatus, "timed-out");
                assert.equal(error.killStatus, "kill-confirmed");
                return true;
            },
        );

        grandchildPid = Number(readFileSync(childPidPath, "utf8"));
        assert.ok(Number.isInteger(grandchildPid) && grandchildPid > 0);
        assert.equal(await waitForProcessExit(grandchildPid, 3_000), true);
        const runRecord = JSON.parse(readFileSync(path.join(timeoutError.runDirectory, "run.json"), "utf8"));
        assert.equal(runRecord.terminalStatus, "timed-out");
        assert.equal(runRecord.killResult.status, "kill-confirmed");
        assert.equal(runRecord.cleanupConfirmed, true);
    } finally {
        if (timeoutError?.runDirectory) {
            const runRecordPath = path.join(timeoutError.runDirectory, "run.json");
            if (existsSync(runRecordPath)) {
                const runRecord = JSON.parse(readFileSync(runRecordPath, "utf8"));
                if (runRecord.rootPid && processIsAlive(runRecord.rootPid)) {
                    await terminateProcessTree(runRecord.rootPid);
                }
            }
            rmSync(timeoutError.runDirectory, { recursive: true, force: true });
        }
        if (grandchildPid && processIsAlive(grandchildPid)) {
            try {
                process.kill(grandchildPid, process.platform === "win32" ? "SIGTERM" : "SIGKILL");
            } catch {}
        }
        rmSync(testDirectory, { recursive: true, force: true });
    }
});
