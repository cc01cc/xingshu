# xingshu - 星枢

## Scope

星枢是面向项目和资源单元的本地索引中枢。当前 Git 是第一种项目类型。`crates/xingshu-core` 是无 Web 依赖的可嵌入 Rust crate；CLI、Axum server 和 Vue WebUI 必须复用核心。

名称说明：星取群星意象，每个项目像一颗独立的星；枢取中枢与索引枢轴之意，将项目星群汇聚、发现、分类和管理。名称受“天枢”等天文意象启发，是本项目自拟组合。

端口登记见 workspace 文档 `../docs/PORT-001-port-registry.md`：WebUI `12680`，API/server `12681`。

文档入口：`docs/AGENTS.md`。

## Safety

- 不在真实参考仓库上执行 pull、move、backup、overwrite 或 reset。
- 实施测试只使用 tempdir 合成仓或每次新建的 staging run。
- `third-party`/`third-party-frozen` 仅在星枢自身操作中执行 `modify_lock`；用户绕过星枢的手动 Git 操作不由星枢拦截。
- staging 复制必须先 dry-run，再校验文件数、大小和 `.git`/bare 元数据。
- 从 `webui/` 安装依赖必须使用 `pnpm --ignore-workspace`，避免向上识别 workspace。
- 日志默认 JSONL stderr；`XINGSHU_LOG_LEVEL`/`RUST_LOG` 控制级别，`XINGSHU_LOG_FILE` 显式启用按日文件日志；不得记录 token/PAT/Bearer。

## Commands

Preferred (mise, mirrors A03-xihe):

```powershell
mise run setup        # pnpm --ignore-workspace install + cargo fetch
mise run dev          # == dev:host → 12681 server + 12680 Vite in parallel
mise run validate     # lint + typecheck + build + test
mise run staging:dev --run-id dev-001   # new .staging/dev-<run-id>
```

Raw equivalents:

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace --release
cd webui
pnpm --ignore-workspace install
pnpm run typecheck
pnpm run build
```

See `mise.toml` for `dev:server/dev:ui/build/test/lint/typecheck/clean/staging`.

## Structure

- `crates/xingshu-core/`: SQLite、scanner、Git backend、policy、puller、mover
- `crates/xingshu-cli/`: `xingshu` binary
- `crates/xingshu-server/`: localhost Axum API and static WebUI host
- `webui/`: Vue/Vite UI
- `scripts/create-test-staging.ps1`: only synthetic staging generator
- `scripts/stage-sample.ps1`: guarded copy helper, dry-run by default
- `docs/`: project documentation and API contract

## Git Remotes

- `origin`: private development repository
- `public`: filtered public release repository

Do not push credentials, staging data, runtime databases, `target/`, or `webui/node_modules/`.

## Backup & Conflict

- `third-party`/`third-party-frozen` 为只读，`modify_lock=1`；`fork/own` 可写。`repo_kind` 首次启发后重扫不覆盖，需 `kind set` 显式改。
- 拉取冲突（工作区脏或非 fast-forward）策略：`Stop` 直给 `ConflictNeedsDecision`，`Abort` 跳过，`Backup` 走 `fs::rename → .bak.<YYYYMMDDHHmmSS.mmm>`（同父目录、原子移动、已存在则 `_1`/`_2` 递增，`scanner` 通过 `*.bak.*`/`*.broken.*` 忽略），`Overwrite` 走 `reset --hard + clean -fd + pull --ff-only`。
- 备份后拉取为**全量重克隆** `git clone <remote_url> <name>`，非增量 `fetch`；无 `remote_url` 时拒绝克隆并保留备份。

## Output Readability

- `webui/src/App.vue` 不直接渲染 `task.resultJson` 原串；`parsedTaskResult / labelResult / labelStatus / formatDuration / taskSummaryText` 解析并中文标签化（成功/已中止/已跳过/失败/冲突需决策），胶囊徽章+表格+扫描统计，`Failed to fetch` 映射为中文服务检查提示。详见 `docs/i18n/zh-Hans/DEV-006-backup-output.md` 与根 skill `readable-output`。
