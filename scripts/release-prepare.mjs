#!/usr/bin/env node

import { randomUUID } from "node:crypto";
import { spawn, spawnSync } from "node:child_process";
import {
    closeSync,
    mkdtempSync,
    openSync,
    readFileSync,
    writeFileSync,
} from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const WORKSPACE_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const FIRST_STABLE_VERSION = "0.1.0";
const CARGO_RELEASE_VERSION = "1.1.7";
const PACKAGE_NAMES = ["xingshu-cli", "xingshu-core", "xingshu-server"];
const BUMP_LEVELS = new Set(["major", "minor", "patch"]);

function envBoolean(value) {
    return value === "true" || value === "1";
}

export function parseOptions(argv, env = process.env) {
    const options = {
        bump: "minor",
        bumpExplicit: false,
        execute: envBoolean(env.usage_execute),
        executeExplicit: envBoolean(env.usage_execute),
        dryRunExplicit: envBoolean(env.usage_dry_run),
        firstRelease: envBoolean(env.usage_first_release),
        help: false,
    };

    const envBump = env.usage_bump?.trim();
    if (envBump) {
        options.bump = envBump;
        options.bumpExplicit = true;
    }

    for (let index = 0; index < argv.length; index += 1) {
        const arg = argv[index];
        if (arg === "--bump") {
            const value = argv[index + 1];
            if (!value || value.startsWith("--")) {
                throw new Error("--bump requires one of: minor, patch, major");
            }
            options.bump = value;
            options.bumpExplicit = true;
            index += 1;
        } else if (arg === "--execute") {
            options.execute = true;
            options.executeExplicit = true;
        } else if (arg === "--dry-run") {
            options.execute = false;
            options.dryRunExplicit = true;
        } else if (arg === "--first-release") {
            options.firstRelease = true;
        } else if (arg === "--help" || arg === "-h") {
            options.help = true;
        } else {
            throw new Error(`unknown option: ${arg}`);
        }
    }

    if (!options.help && !BUMP_LEVELS.has(options.bump)) {
        throw new Error(`invalid bump '${options.bump}'; supported values are minor, patch, and major`);
    }
    if (options.executeExplicit && options.dryRunExplicit) {
        throw new Error("--dry-run and --execute cannot be used together");
    }
    if (options.firstRelease && options.bumpExplicit) {
        throw new Error("--first-release cannot be combined with an explicit --bump");
    }

    return options;
}

export function planVersionStep(version, options) {
    if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/.test(version)) {
        throw new Error(`workspace version is not a SemVer version: ${version}`);
    }

    if (options.firstRelease) {
        if (version !== FIRST_STABLE_VERSION) {
            throw new Error(
                `first release requires workspace version ${FIRST_STABLE_VERSION}; found ${version}`,
            );
        }
        return {
            action: "keep-first-release-version",
            version,
            execute: options.execute,
            args: [],
        };
    }

    const major = Number(version.split(".")[0]);
    if (options.bump === "major" && major === 0) {
        throw new Error("major bumps are not allowed before 1.0.0");
    }

    return {
        action: "cargo-release-version-step",
        version,
        bump: options.bump,
        execute: options.execute,
        args: [
            "release",
            "version",
            options.bump,
            "--workspace",
            // cargo-release 1.1.7 asks "? [y/N]" before writing. `runFileLogged` gives the
            // child an empty stdin, so the prompt reads EOF, the answer is "no", and the
            // process still exits 0 with the manifests untouched (a silent no-op that used
            // to be recorded as success). `--no-confirm` is the documented way to skip that
            // prompt (see `cargo release version --help`). Dry runs keep the preview prompt
            // semantics by never passing the write flags at all.
            ...(options.execute ? ["--execute", "--no-confirm"] : []),
        ],
    };
}

function processExists(pid, processGroup) {
    if (!pid) return false;
    try {
        process.kill(processGroup ? -pid : pid, 0);
        return true;
    } catch (error) {
        if (error?.code === "ESRCH") return false;
        if (error?.code === "EPERM") return true;
        throw error;
    }
}

async function waitForProcessExit(pid, { processGroup = false, timeoutMs }) {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
        if (!processExists(pid, processGroup)) return true;
        await new Promise((resolve) => setTimeout(resolve, 50));
    }
    return !processExists(pid, processGroup);
}

export async function terminateProcessTree(pid) {
    if (!pid) {
        return { status: "kill-unconfirmed", method: "none", reason: "child PID unavailable" };
    }

    if (process.platform === "win32") {
        const result = spawnSync("taskkill.exe", ["/PID", String(pid), "/T", "/F"], {
            stdio: "ignore",
            timeout: 5_000,
            windowsHide: true,
        });
        const rootExited = await waitForProcessExit(pid, { timeoutMs: 2_000 });
        return {
            status: result.status === 0 && rootExited ? "kill-confirmed" : "kill-unconfirmed",
            method: "taskkill /T /F",
            exitCode: result.status,
            rootExited,
            error: result.error?.message,
        };
    }

    try {
        process.kill(-pid, "SIGTERM");
    } catch (error) {
        if (error?.code === "ESRCH") {
            return { status: "kill-confirmed", method: "process-group SIGTERM", groupExited: true };
        }
        return {
            status: "kill-unconfirmed",
            method: "process-group SIGTERM",
            error: error?.message ?? String(error),
        };
    }

    if (await waitForProcessExit(pid, { processGroup: true, timeoutMs: 1_500 })) {
        return { status: "kill-confirmed", method: "process-group SIGTERM", groupExited: true };
    }

    try {
        process.kill(-pid, "SIGKILL");
    } catch (error) {
        if (error?.code !== "ESRCH") {
            return {
                status: "kill-unconfirmed",
                method: "process-group SIGKILL",
                error: error?.message ?? String(error),
            };
        }
    }
    const groupExited = await waitForProcessExit(pid, { processGroup: true, timeoutMs: 3_000 });
    return {
        status: groupExited ? "kill-confirmed" : "kill-unconfirmed",
        method: "process-group SIGKILL",
        groupExited,
    };
}

function writeRunRecord(recordPath, record) {
    writeFileSync(recordPath, `${JSON.stringify(record, null, 2)}\n`, { encoding: "utf8" });
}

function closeOutputFiles(stdoutFd, stderrFd) {
    for (const fd of [stdoutFd, stderrFd]) {
        try {
            closeSync(fd);
        } catch (error) {
            // A second close on a failure path must stay harmless: letting EBADF escape
            // would replace the failure that actually happened and hide its cause.
            if (error?.code !== "EBADF") throw error;
        }
    }
}

export function cargoReleaseMissingError(command, args, stderr, runDirectory) {
    if (!(command === "cargo" && args[0] === "release" && stderr.includes("no such command"))) {
        return null;
    }
    const runRecordPath = path.join(runDirectory ?? "", "run.json");
    const error = new Error(
        `cargo-release ${CARGO_RELEASE_VERSION} is required for future minor/patch/major bumps but is not installed; no version files were changed; run record: ${runRecordPath}`,
    );
    error.runDirectory = runDirectory;
    error.terminalStatus = "failed";
    return error;
}

export async function runFileLogged(command, args, { cwd, timeoutMs, env, onRecord } = {}) {
    const runId = randomUUID();
    const outputDir = mkdtempSync(path.join(os.tmpdir(), `xs-release-prepare-${runId}-`));
    const stdoutPath = path.join(outputDir, "stdout.log");
    const stderrPath = path.join(outputDir, "stderr.log");
    const runRecordPath = path.join(outputDir, "run.json");
    const stdoutFd = openSync(stdoutPath, "w");
    const stderrFd = openSync(stderrPath, "w");
    const startedAt = new Date();
    const record = {
        runId,
        cwd,
        startedAt: startedAt.toISOString(),
        deadlineAt: new Date(startedAt.getTime() + timeoutMs).toISOString(),
        timeoutMs,
        rootPid: null,
        command: [command, ...args],
        stdoutPath,
        stderrPath,
        terminalStatus: "running",
        exitCode: null,
        killResult: null,
        cleanupConfirmed: null,
    };

    let child;
    let spawnError = null;
    let closeOutput = true;
    let timer;
    let resolveClosed;
    const closed = new Promise((resolve) => {
        resolveClosed = resolve;
    });

    try {
        child = spawn(command, args, {
            cwd,
            env: env ? { ...process.env, ...env } : process.env,
            stdio: ["ignore", stdoutFd, stderrFd],
            detached: process.platform !== "win32",
            windowsHide: true,
        });
        record.rootPid = child.pid ?? null;
        writeRunRecord(runRecordPath, record);

        child.once("error", (error) => {
            spawnError = error;
            resolveClosed({ code: null, signal: null });
        });
        child.once("close", (code, signal) => resolveClosed({ code, signal }));

        const timeout = new Promise((resolve) => {
            timer = setTimeout(() => resolve(null), timeoutMs);
        });
        const firstResult = await Promise.race([closed, timeout]);
        clearTimeout(timer);

        if (firstResult === null) {
            let killResult;
            try {
                killResult = await terminateProcessTree(child.pid);
            } catch (error) {
                killResult = {
                    status: "kill-unconfirmed",
                    method: process.platform === "win32" ? "taskkill /T /F" : "process-group kill",
                    error: error?.message ?? String(error),
                };
            }
            const finalResult = await Promise.race([
                closed,
                new Promise((resolve) => setTimeout(() => resolve(null), 5_000)),
            ]);
            if (finalResult === null) {
                killResult.status = "kill-unconfirmed";
            }
            record.terminalStatus = killResult.status === "kill-confirmed" ? "timed-out" : "kill-unconfirmed";
            record.killResult = killResult;
            record.cleanupConfirmed = killResult.status === "kill-confirmed" && finalResult !== null;
            record.exitCode = finalResult?.code ?? null;
            record.endedAt = new Date().toISOString();
            record.elapsedMs = Date.now() - startedAt.getTime();
            writeRunRecord(runRecordPath, record);
            closeOutputFiles(stdoutFd, stderrFd);
            closeOutput = false;

            const stdout = readFileSync(stdoutPath, "utf8");
            const stderr = readFileSync(stderrPath, "utf8");
            if (stdout) process.stdout.write(stdout);
            if (stderr) process.stderr.write(stderr);
            const result = new Error(
                `${command} timed out after ${timeoutMs}ms; process-tree ${record.killResult.status}; run record: ${runRecordPath}`,
            );
            result.runDirectory = outputDir;
            result.terminalStatus = record.terminalStatus;
            result.killStatus = record.killResult.status;
            throw result;
        }

        record.exitCode = firstResult.code;
        record.signal = firstResult.signal;
        record.terminalStatus = spawnError || firstResult.code !== 0 ? "failed" : "completed";
        record.cleanupConfirmed = true;
        record.endedAt = new Date().toISOString();
        record.elapsedMs = Date.now() - startedAt.getTime();
        writeRunRecord(runRecordPath, record);
        // Callers may need to annotate the completed record (for example when the child
        // reported success but its intended side effect never happened).
        onRecord?.({ path: runRecordPath, record });
        closeOutputFiles(stdoutFd, stderrFd);
        closeOutput = false;

        const stdout = readFileSync(stdoutPath, "utf8");
        const stderr = readFileSync(stderrPath, "utf8");
        if (spawnError) {
            if (stderr) process.stderr.write(stderr);
            const error = new Error(`${command} could not run: ${spawnError.message}; run record: ${runRecordPath}`);
            error.runDirectory = outputDir;
            error.terminalStatus = "failed";
            throw error;
        }
        if (firstResult.code !== 0) {
            if (stdout) process.stdout.write(stdout);
            if (stderr) process.stderr.write(stderr);
            const missingCargoRelease = cargoReleaseMissingError(command, args, stderr, outputDir);
            if (missingCargoRelease) throw missingCargoRelease;
            const error = new Error(
                `${command} exited with status ${firstResult.code ?? "unknown"}; run record: ${runRecordPath}`,
            );
            error.runDirectory = outputDir;
            error.terminalStatus = "failed";
            throw error;
        }

        // The run record outlives the process on purpose: a failed or no-op bump is
        // exactly the case where `T1.2` needs the record path, and deleting it would
        // erase the only evidence of what cargo-release actually did.
        onRecord?.({ path: runRecordPath, record });
        return stdout;
    } catch (error) {
        if (timer) clearTimeout(timer);
        if (closeOutput) closeOutputFiles(stdoutFd, stderrFd);
        throw error;
    }
}

export function assertCargoReleaseVersion(output) {
    const match = output.match(/(?:cargo-release|cargo release)\s+v?(\d+\.\d+\.\d+)/i);
    if (!match) {
        throw new Error("could not determine the installed cargo-release version");
    }
    if (match[1] !== CARGO_RELEASE_VERSION) {
        throw new Error(
            `cargo-release ${CARGO_RELEASE_VERSION} is required for future version bumps; found ${match[1]}`,
        );
    }
    return match[1];
}

async function checkCargoReleaseVersion({ cwd, run }) {
    const output = await run("cargo", ["release", "--version"], { cwd, timeoutMs: 15_000 });
    return assertCargoReleaseVersion(output);
}

export async function readWorkspaceVersion({ cwd = WORKSPACE_ROOT, run = runFileLogged } = {}) {
    const metadataText = await run(
        "cargo",
        ["metadata", "--no-deps", "--locked", "--format-version", "1"],
        { cwd, timeoutMs: 60_000 },
    );
    const metadata = JSON.parse(metadataText);
    const members = new Set(metadata.workspace_members);
    const packages = metadata.packages.filter(
        (pkg) => members.has(pkg.id) && PACKAGE_NAMES.includes(pkg.name),
    );
    const names = new Set(packages.map((pkg) => pkg.name));
    const versions = new Set(packages.map((pkg) => pkg.version));

    if (packages.length !== PACKAGE_NAMES.length || names.size !== PACKAGE_NAMES.length) {
        throw new Error(
            `workspace package inventory mismatch; expected ${PACKAGE_NAMES.join(", ")}`,
        );
    }
    if (versions.size !== 1) {
        throw new Error("XS workspace packages do not share one version");
    }

    return packages[0].version;
}

async function assertCleanForVersionWrite(cwd, run) {
    const status = await run("git", ["status", "--porcelain", "--untracked-files=all"], {
        cwd,
        timeoutMs: 15_000,
    });
    if (status.trim()) {
        throw new Error(
            "refusing to change versions in a dirty worktree; review/commit or isolate the changes first",
        );
    }
}

function printHelp(log = console.log) {
    log(`Usage: node scripts/release-prepare.mjs [options]

Options:
  --first-release   Keep the initial 0.1.0 version unchanged
  --bump <level>    Select minor (default), patch, or major (major requires 1.0+)
  --execute         Apply the cargo-release version step (default: dry-run)
  --dry-run         Explicitly preview only`);
}

export async function prepareRelease({
    argv = process.argv.slice(2),
    env = process.env,
    cwd = WORKSPACE_ROOT,
    run = runFileLogged,
    log = console.log,
} = {}) {
    const options = parseOptions(argv, env);
    if (options.help) {
        printHelp(log);
        return { action: "help" };
    }

    const version = await readWorkspaceVersion({ cwd, run });
    const plan = planVersionStep(version, options);
    if (plan.action === "keep-first-release-version") {
        log(
            `First stable release: workspace version is already ${FIRST_STABLE_VERSION}; no version bump or Cargo release command will run.`,
        );
        return plan;
    }

    log(
        `Workspace ${version} → ${options.bump} bump (${options.execute ? "execute" : "dry-run"}).`,
    );
    const cargoReleaseVersion = await checkCargoReleaseVersion({ cwd, run });
    if (options.execute) await assertCleanForVersionWrite(cwd, run);

    log(
        `Running cargo ${plan.args.join(" ")} with cargo-release ${cargoReleaseVersion}. No publish, tag, commit, or push step is invoked.`,
    );
    let versionStep;
    const output = await run("cargo", plan.args, {
        cwd,
        timeoutMs: 120_000,
        onRecord(record) {
            versionStep = record;
        },
    });
    if (output) log(output.trimEnd());

    if (!options.execute) return plan;

    // Anti-false-green check: "the tool exited 0" is not evidence that it did anything.
    // cargo-release can end successfully after declining its own confirmation prompt, so
    // the version is re-read from the manifests and compared with the pre-run value.
    const appliedVersion = await readWorkspaceVersion({ cwd, run });
    if (appliedVersion === version) {
        const detail = `cargo ${plan.args.join(" ")} exited 0 but the workspace version is still ${version}`;
        const noOpRecordPath = markNoOpRecord(versionStep, detail);
        const error = new Error(
            `${detail}; refusing to report success${noOpRecordPath ? ` (run record: ${noOpRecordPath})` : ""}`,
        );
        error.terminalStatus = "failed";
        if (noOpRecordPath) error.runDirectory = path.dirname(noOpRecordPath);
        throw error;
    }

    log(
        `Workspace version ${version} → ${appliedVersion}; run record: ${versionStep?.path ?? "unavailable"}`,
    );
    return plan;
}

function markNoOpRecord(versionStep, detail) {
    if (!versionStep?.path || !versionStep.record) return null;
    const failed = {
        ...versionStep.record,
        terminalStatus: "failed",
        outcome: "no-op",
        detail,
    };
    writeRunRecord(versionStep.path, failed);
    return versionStep.path;
}

async function main() {
    try {
        await prepareRelease();
    } catch (error) {
        console.error(`release-prepare: ${error.message}`);
        if (error.killStatus) console.error(`release-prepare process-tree kill status: ${error.killStatus}`);
        process.exitCode = 1;
    }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
    void main();
}
