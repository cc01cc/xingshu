# 星枢 Xingshu

星枢是项目与资源的本地索引和管理中枢。当前首先支持 Git 仓库，面向分布在多个目录、磁盘、组织和远程上的大量项目，提供 SQLite 元数据、领域标签、差异化更新策略、冲突保护和本地 WebUI。

“星”取群星意象：每个项目或资源单元像一颗独立的星，彼此独立却共同构成项目星群；“枢”取中枢、枢纽与索引枢轴之意，表达将这些群星汇聚、发现、索引、分类和管理。星枢是项目自拟的原创组合，受“天枢”等天文意象启发，但不是古籍固定专名。英文释义为 *the stellar hub*。

## Current Status

首版核心、CLI、API、基础 WebUI、异步任务实时进度、结构化日志、分层测试和正式 Playwright 验证已完成。近期增量（PLAN-0297/0314）：拉取冲突三态面板与 CLI 决策 prompt、只读状态巡检（本地 + `ls-remote` 远端漂移）、独立备份（快照/归档/清理）、增量扫描，以及上述能力的 WebUI 与 API 收口。管理面增强（PLAN-0315/0316）：WebUI 结构拆分（App.vue 薄壳化）、冲突明细只读 diff 弹层、策略生效来源可视化与标签策略编辑、搬迁目标空间预检。PLAN-0617 补入了 CLI 远端 clone 到注册 root；API/WebUI caller 后续承接。

## CLI Release Status

首个稳定 CLI 版本号为 `0.1.0`。XS GitHub Releases 页面列出稳定资产和当前可用版本；只有页面列出的二进制适用于对应平台。

候选快照 push 到公开仓 `release-candidate/v0.1.0` 分支后自动触发候选构建；不依赖 `workflow_dispatch` 或本地 GH CLI。候选只为 Windows x86_64 和 Linux x86_64 生成；最低运行环境为 Windows 10+（Rust MSVC target baseline）和 Linux GNU glibc 2.35+（Ubuntu 22.04 baseline）。只有候选 run 提供安装/运行证据后，这些平台下限才算验收；Windows release target 静态链接 MSVC CRT；候选 CI 执行 `dumpbin /dependents`，若仍导入 VCRUNTIME/MSVCP/UCRT 则失败。manifest 记录 `crtLinkage=static` 与运行依赖：Windows 10+ 和 Git on PATH，Linux glibc 2.35+、Git 和 `df` on PATH（`df` 由 GNU coreutils 提供，用于移动前的剩余空间检查）；Linux manifest 还记录 runner 上 `ldd` 得到的动态库 SONAME 列表，并在有 unresolved dependency 时失败。tag workflow 按 target allowlist 校验 manifest 字段；候选导入检查通过后，Windows 不需要 VC++ Redistributable。实际可用资产和支持平台以 Releases 页面为准。每次运行的 artifact 分别命名为 `xingshu-candidate-x86_64-pc-windows-msvc-attempt-<run_attempt>` 和 `xingshu-candidate-x86_64-unknown-linux-gnu-attempt-<run_attempt>`，保留 90 天且每次 attempt 独立不可变。每个 artifact 对应同一 `github.sha` 和 `candidateRunAttempt`，并含稳定资产命名的归档（`xingshu-v0.1.0-x86_64-pc-windows-msvc.zip` 或 `xingshu-v0.1.0-x86_64-unknown-linux-gnu.tar.gz`）、匹配归档的 `.sha256` 文件及 `xingshu-v0.1.0-<target>.manifest.json`；manifest 记录 `minimumOsAbi`、`buildRunner`、`crtLinkage` 和 `runtimeDependencies`。候选 artifact 仅包含每归档校验文件；稳定 GitHub Release 还提供汇总校验文件 `SHA256SUMS`。单个归档的直接校验仍对照其相邻的 `.sha256` 文件。Windows 的值为 `Windows 10+` / `windows-2025`，Linux GNU 为 `glibc 2.35+` / `ubuntu-22.04`。候选和稳定 manifest 的 `signatureStatus` 均为 `unsigned`，发布资产不做代码签名。Windows 可能显示 SmartScreen/未知发布者警告，Linux 归档也未签名。`.sha256` 与 `SHA256SUMS` 只校验文件完整性，不认证发布者身份。候选显示 `0.1.0`，不创建 `-rc` 版本或 tag；artifact 只用于验证，**不是公开 Release**。macOS 与 Linux arm64 不属于 `v0.1.0` 用户支持目标。

候选验收并获最终用户批准后，release owner 将同一候选 commit fast-forward 到公开 `main` 并 push `v0.1.0` tag。稳定发布 workflow 只接受不带 prerelease 后缀的稳定 SemVer tag；它会从 `release-candidate/v0.1.0` 查找相同 source SHA、候选 workflow 与 `run_attempt` 的成功候选，下载该 attempt 的不可变 artifact，核验 manifest、哈希及 `minimumOsAbi`/`buildRunner`/`crtLinkage`/`runtimeDependencies` 是否符合 target allowlist，再用原始资产创建 GitHub Release；不重建资产，也不依赖手动 run ID 输入或推送私有 `origin`。

当前源码已包含 `mise run release:prepare`：默认 dry-run + minor；维护者显式指定 `--bump patch` 时选择 patch，只有显式 `--execute` 才改版本。首发 `--first-release` 验证 workspace 为 `0.1.0` 后 no-op，不需要 `cargo-release`。非首发 bump 需维护者手动准备 `cargo-release` 0.25.20 并加入 PATH；流程不自动安装。workspace 仍为 0.x 时拒绝 `--bump major`；达到 1.0+ 后可按标准 SemVer 显式选择 patch 或 major，默认仍为 minor。0.x 到 1.0 是独立的稳定性决策/PLAN，要求一次性手动修改 workspace 版本；不是常规自动 bump，当前没有 `--version` 参数。该任务只调用 Cargo workspace version step，不发布 crate、不打 tag、不 push。Agent 不会自动把默认 minor 降为 patch。候选 Actions artifacts 只用于验证；正式 CLI 二进制与当前可用版本以 GitHub Releases 页面为准。

CLI 运行时需要系统 Git，并要求 `git` 可从命令行找到。当前源码包含 CLI 专属环境解析及 `xs db path`。目标中，正式 CLI 使用当前用户的数据目录，源码开发 CLI 使用 `.staging`；server 保持既有数据库路径契约。正式 CLI 二进制和当前平台可用性以 GitHub Releases 页面为准；详见 CLI 配置文档。

## Quick Start

```powershell
cargo build --workspace --release
pwsh -NoProfile -File .\scripts\create-test-staging.ps1 -TargetRoot .\.staging\run-001
.\target\release\xs.exe --db .\.staging\run-001\xingshu-dev.db roots add .\.staging\run-001\root-a
.\target\release\xs.exe --db .\.staging\run-001\xingshu-dev.db roots add .\.staging\run-001\root-b
.\target\release\xs.exe --db .\.staging\run-001\xingshu-dev.db scan --my-org cc01cc
```

启动本地 API 与构建后的 WebUI：

```powershell
$env:XINGSHU_DB = ".staging\run-001\xingshu-dev.db"
$env:XINGSHU_PORT = "12681"
.\target\release\xingshu-server.exe
```

- API/WebUI：<http://127.0.0.1:12681>
- Health：<http://127.0.0.1:12681/health>
- Vite dev：在 `webui/` 执行 `pnpm --ignore-workspace install` 后运行 `pnpm run dev`，端口 `12680`

异步 scan/pull API：

```powershell
$task = Invoke-RestMethod -Method Post -Uri http://127.0.0.1:12681/api/v1/tasks -ContentType "application/json" -Body '{"type":"scan"}'
Invoke-RestMethod "http://127.0.0.1:12681/api/v1/tasks/$($task.taskId)"
```

批量 pull 可请求人工冲突决策：`{"type":"pull","conflictMode":"ask"}`。通过 `GET /api/v1/tasks/{taskId}/conflicts` 查询冲突，再向 `POST /api/v1/tasks/{taskId}/repos/{repoId}/decision` 提交 `backup`、`overwrite` 或 `skip`（旧值 `abort` 兼容）。实时进度通过 `GET /api/v1/tasks/{taskId}/stream` 的 SSE 返回；同步 `POST /api/v1/scan` 和单仓 pull 仍保留给脚本兼容。

## CLI

```text
xs roots add <path> [--disk LABEL] [--priority N]
xs roots list | rm <id>
xs db path
xs [--db <DB_PATH>] clone --root <ROOT_ID> --dest <RELATIVE_PATH> [--my-org <ORG>]... <REMOTE>
xs scan [--root PATH]... [--include-nested] [--max-depth N] [--my-org ORG]... [--full]
xs list [query] [--tag TAG] [--root ID] [--status STATUS] [--org ORG]
xs tag <repo> <tag...> | untag <repo> <tag...>
xs kind set <repo> <third-party|third-party-frozen|fork|own> [--upstream REMOTE]
xs policy set <repo> [--strategy STRATEGY] [--conflict ACTION] [--unattended ACTION]
xs policy set --tag TAG [--strategy STRATEGY] [--conflict ACTION] [--unattended ACTION]
xs pull [--jobs N] [--unattended] [--root ID] [--dry-run] [--fetch]
xs move <repo> <root-id>
xs stats [--by root|tag|org]
xs status [--root ID] [--repo NAME] [--remote]
xs backup create <repo> [--dest DIR] [--archive tar.gz|tar.zst] [--keep N]
xs backup list <repo> [--dest DIR]
xs backup restore <repo> <backup-path>
xs backup prune <repo> --keep N [--execute]
```

## CLI 数据库备份与恢复

上面的 `xs backup` 命令备份的是 Git 仓库快照，不是 CLI 数据库。用 `xs db path` 确认 CLI 当前选中的数据库。官方发行版的默认路径为：Windows `%LOCALAPPDATA%/Xingshu/xingshu-prod.db`，Linux `${XDG_DATA_HOME:-~/.local/share}/xingshu/xingshu-prod.db`；dev 库在相同数据根下的 `dev/xingshu-dev.db`。源码 CLI 的 dev 库仍使用 `.staging`。macOS 不在 v0.1.0 用户支持矩阵中。

冷备份前，停止所有可能写入该数据库的 CLI 命令和 server 进程。将 `.db` 文件与同名的 `-wal`、`-shm` sidecar（若存在）作为一组复制，保持文件主名一致。FTS 搜索 sidecar 是可重建的派生缓存，不必纳入备份；需要时可用 `xs --db <RESTORED_DB> index rebuild` 重建。

恢复时，先把整组文件复制到隔离 profile 或备用数据库路径。用 `xs --db <RESTORED_DB> db path`、`xs --db <RESTORED_DB> roots list` 和 `xs --db <RESTORED_DB> stats` 验证；如需可重建 FTS 索引。确认后再替换活动库，替换前仍须停止所有写入进程。卸载 CLI 只移除程序；删除用户数据是另一个需要明确执行的操作。

Full command reference with arguments, outputs, and exit codes: [`docs/i18n/zh-Hans/DEV-002-cli-config.md`](docs/i18n/zh-Hans/DEV-002-cli-config.md) / [`docs/i18n/en/DEV-002-cli-config.md`](docs/i18n/en/DEV-002-cli-config.md). `scan` prints its JSON report to stdout and progress to stderr; `pull` writes per-repo results to stdout and may exit `0` even when individual repositories fail (check output or `fetch_log`).

The `clone` command uses `xs [--db <DB_PATH>] clone --root <ROOT_ID> --dest <RELATIVE_PATH> [--my-org <ORG>]... <REMOTE>`. The global `--db` may also appear after `clone`. It writes into a new directory under an existing registered root and registers only that repository. The destination parent must already exist and contain no symlink/junction; existing destinations/stale registrations and paths inside another registered root are rejected. Private remotes use the system Git credential helper or SSH agent; credential-bearing URLs and raw Git error output are rejected/suppressed.

**PLAN-0587 search/index commands are implemented in this build.** The v1 command contract uses `xs` as the only executable name; there is no `xingshu` executable alias. `index status|update|rebuild` manages a derived SQLite FTS5 trigram index, while `search` and `files` query indexed HEAD content and paths. `grep` scans selected worktree text directly, including tracked modifications and non-ignored untracked files; optional `--ref` reads one local Git revision without checkout. `read` reads a bounded text file from HEAD or a local ref. Search and index status can aggregate same-key repositories across selected roots and report `rootId`; single-target operations such as `read` and index update/rebuild require `--root` when a repo selector is ambiguous. Search/grep default to 1,000 match lines and files to 10,000 paths (maximum configurable cap 100,000); truncation is a successful but incomplete result. Searches are read-only: no fetch, pull, checkout, or repository writes; they do not follow symlinks, search `.git` objects or binary contents, or implicitly traverse unregistered roots. `search` never scans a worktree and reports missing/stale indexes instead. See the full contract in the CLI reference.

## Safety

- `third-party` 和 `third-party-frozen` 只在星枢自身操作中锁定修改；用户直接使用 Git 不由星枢拦截。
- pull 冲突默认停止并交给用户决定；无人值守默认 `skip`（旧值 `abort` 兼容）。
- 测试和开发写操作只允许使用 tempdir 或独立 staging；不得对真实参考仓库执行 pull、move、backup 或 overwrite。
- Clone 仅接受标准 HTTP(S)、SSH/SCP-like、file URL；拒绝嵌入凭据的 URL，私有仓库使用系统 Git credential helper 或 SSH agent。日志/错误不得包含 token、PAT、Bearer、原始 Git stderr 或完整环境变量。

## Logging

运行日志默认输出 JSONL 到 stderr。设置 `XINGSHU_LOG_LEVEL`（或回退到 `RUST_LOG`）控制级别；设置 `XINGSHU_LOG_FILE` 可启用按日/大小轮转的 JSONL 文件日志，默认单文件 10 MiB、保留 7 个轮转文件，可用 `XINGSHU_LOG_MAX_BYTES`/`XINGSHU_LOG_MAX_FILES` 覆盖。`fetch_log` 是 SQLite 中的 pull 审计，不是运行日志替代品。

## Project Documentation

- Product contract index: [spec/README.md](spec/README.md) (accepted target contracts; implementation status is tracked separately)
- 文档索引：`docs/AGENTS.md`
- 中文文档：`docs/i18n/zh-Hans/README.md`
- English docs: `docs/i18n/en/README.md`
- API contract: `docs/api/openapi.yaml`
- Ports: WebUI `12680`，API/server `12681`

## Development

### Development Mode (mise, recommended)

```powershell
# 1. Install deps (Rust + WebUI)
mise run setup

# 2. Create a fresh staging run (synthetic; never writes to reference repositories)
mise run staging:dev --run-id dev-001
# (default DB is now .staging/dev-001/xingshu-dev.db; set $env:XINGSHU_DB only to switch runs)
# Prod DB: mise run prod:server (XINGSHU_ENV=prod, ./xingshu-prod.db)

# 3. Start server (12681) + Vite (12680, proxies /api to 12681) in parallel
mise run dev          # == mise run dev:host, Ctrl+C cleans up both
```

- Vite: http://127.0.0.1:12680  ·  API/built WebUI: http://127.0.0.1:12681  ·  Health: http://127.0.0.1:12681/health
- Add roots in Settings → click **扫描索引** (or `POST /api/v1/tasks {type:scan}`); repos appear in the repository table.
- Raw cargo/pnpm equivalent (two terminals):

```powershell
$env:XINGSHU_PORT = "12681"
cargo run -p xingshu-server
# terminal 2
cd webui
pnpm --ignore-workspace run dev -- --host 127.0.0.1
```

### Validation

```powershell
mise run validate      # lint + typecheck + build + test
# or bare commands
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace --release
cd webui
pnpm --ignore-workspace install
pnpm run typecheck
pnpm run build
```

Other mise entries: `mise run dev:server/dev:ui/setup/build/test/lint/typecheck/clean/staging` — see `mise.toml`.
