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
mise run staging:dev --run-id dev-001   # new .staging/<run-id>, default dev-001
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

## Staging 与 E2E（真实数据手册）

- 数据库文件名统一规范 `xingshu-<env>.db`：dev 默认 `.staging/dev-001/xingshu-dev.db`，prod 默认 `./xingshu-prod.db`；优先级为显式 `--db` > `XINGSHU_DB` > 默认值；prod 下 `.staging/` 路径拒绝启动；server 启动首行打印实际库路径。
- 真实数据基线（只读 `S:\zeogit-ref`，绝不写源）：`real-mini-001`（6 真仓约 963MB）与 `real-edge-001`（9 仓，含 bare/nested/bak/dirty）均位于 `.staging/`（已 gitignore）；双 DB（`real-mini-001/xingshu-dev.db`、`real-edge-001/xingshu-dev.db`）为回归基线。
- 重建：`pwsh scripts/create-real-test-staging.ps1 [-WithEdgeCases]`（底层走 `stage-sample.ps1 -Repository @(...) -Execute`，拷贝后校验 `.git` 与文件数，再 `roots add` + `scan`）。
- 手动测试：缺省即进 dev 库，直接 `cargo run -p xingshu-server`（12681）+ Vite `pnpm --ignore-workspace run dev`（host/port 以 `webui/vite.config.ts` 为准：`127.0.0.1:12680`）；浏览器打开 `http://127.0.0.1:12680`。换 run 才设 `$env:XINGSHU_DB`；生产库用 `mise run prod:server`。

## E2E 排障三件套

- `validate-plan.py` 路径：`uv run .agents/skills/plan-mode/scripts/validate-plan.py plans/PLAN-XXX.md`（workspace 根执行）。
- `playwright-cli` 直接调用（禁 `npx` 前缀，防 npm 初始化延迟与 `Unknown project config` 噪音）；每条命令带 bash timeout。
- `e2e/playwright.config.ts` 的 `webServer.cwd` 必须指向项目根，否则 `ServeDir webui/dist` 相对路径 404；`webServer` 会重建 `.staging/e2e-playwright` 合成 5 仓。
- **git 组合参数拆分**（PLAN-250 实测）：`run_git` 的 args 数组中，带空格的组合 revspec（如 `"HEAD ^<base>"`）会被 git 当作单个无效参数静默失败（`rev-list --count` 返回 0）——必须拆成独立元素 `&["rev-list", "--count", from, &format!("^{base}")]`。
- **git cherry 方向**：`git cherry <upstream> <head>` 列出 head 侧提交，`-` 行为 patch 等价（上游已有同内容提交）；配对本地/上游提交用 patch-id 反查，不要假设方向。

## Structure

- `crates/xingshu-core/`: SQLite、scanner、Git backend、policy、puller、mover
- `crates/xingshu-cli/`: `xingshu` binary
- `crates/xingshu-server/`: localhost Axum API and static WebUI host
- `webui/`: Vue/Vite + Tailwind v4 + shadcn-vue UI（组件 vendor 于 `src/components/ui/`，主题 token 见 `src/shadcn-theme.css`；新依赖用 `pnpm --ignore-workspace add <pkg>@<version>` 并过 7 天冷却与构建脚本评审）
- `e2e/`: Playwright CLI E2E（`playwright.config.ts` + `tests/xingshu.spec.ts` 15 用例 + `scripts/start-e2e-server.ps1`；含 dirty/diverged 冲突面板、open API、favicon 用例）
- `scripts/create-test-staging.ps1`: only synthetic staging generator
- `scripts/create-real-test-staging.ps1`: real-git staging helper（读 `S:\zeogit-ref`，写 `.staging/`）
- `scripts/stage-sample.ps1`: guarded copy helper, dry-run by default
- `docs/`: project documentation and API contract

## Git Remotes

- `origin`: private development repository
- `public`: filtered public release repository

Do not push credentials, staging data, runtime databases, `target/`, or `webui/node_modules/`.

## Backup & Conflict

- `third-party`/`third-party-frozen` 为只读，`modify_lock=1`；`fork/own` 可写。`repo_kind` 首次启发后重扫不覆盖，需 `kind set` 显式改。
- 冲突三态（PLAN-250）：`dirty`（工作区变更，面板含 status 明细+后果对照）、`diverged/non-ff`（fetch 后分叉，双栏提交清单+`git cherry` ≡ 补丁等价标记）、`ahead_clean`（纯领先不进面板，`pull` 记 `ahead`，列表显示"本地领先"徽标）。离线回退旧判定。
- 拉取冲突（工作区脏或非 fast-forward）策略：`Stop` 直给 `ConflictNeedsDecision`（携带 `ConflictInfo`），`Abort` 跳过，`Backup` 走 `fs::rename → .bak.<YYYYMMDDHHmmSS.mmm>`（同父目录、原子移动、已存在则 `_1`/`_2` 递增，`scanner` 通过 `*.bak.*`/`*.broken.*` 忽略），`Overwrite` 走 `reset --hard + clean -fd + pull --ff-only`（UI 强制二次确认）。
- `POST /api/v1/open`：打开仓库/备份目录（canonicalize + 已注册 root 白名单，防 CSRF 越界）。
- 备份后拉取为**全量重克隆** `git clone <remote_url> <name>`，非增量 `fetch`；无 `remote_url` 时拒绝克隆并保留备份。

## Output Readability

- `webui/src/App.vue` 不直接渲染 `task.resultJson` 原串；`parsedTaskResult / labelResult / labelStatus / formatDuration / taskSummaryText` 解析并中文标签化（成功/已中止/已跳过/失败/冲突需决策），胶囊徽章+表格+扫描统计，`Failed to fetch` 映射为中文服务检查提示。详见 `docs/i18n/zh-Hans/DEV-006-backup-output.md` 与根 skill `readable-output`。
