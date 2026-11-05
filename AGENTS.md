# xingshu - 星枢

## Scope

星枢是面向项目和资源单元的本地索引中枢。当前 Git 是第一种项目类型。`crates/xingshu-core` 是无 Web 依赖的可嵌入 Rust crate；CLI、Axum server 和 Vue WebUI 必须复用核心。

名称说明：星取群星意象，每个项目像一颗独立的星；枢取中枢与索引枢轴之意，将项目星群汇聚、发现、分类和管理。名称受“天枢”等天文意象启发，是本项目自拟组合。

端口约定：WebUI `12680`，API/server `12681`。

文档入口：`docs/AGENTS.md`。

产品契约入口：`spec/README.md`。根级 SPEC 描述已接受的目标契约；`active` 不代表能力已完整实现，当前实现状态以契约元数据和源码为准。

## Safety

- 不在真实参考仓库上执行 pull、move、backup、overwrite 或 reset。
- 实施测试只使用 tempdir 合成仓或每次新建的 staging run。
- `third-party`/`third-party-frozen` 仅在星枢自身操作中执行 `modify_lock`；用户绕过星枢的手动 Git 操作不由星枢拦截。
- staging 复制必须先 dry-run，再校验文件数、大小和 `.git`/bare 元数据。
- 从 `webui/` 安装依赖必须使用 `pnpm --ignore-workspace`，避免向上识别 workspace。
- 日志默认 JSONL stderr；`XINGSHU_LOG_LEVEL`/`RUST_LOG` 控制级别，`XINGSHU_LOG_FILE` 显式启用按日文件日志；不得记录 token/PAT/Bearer。
- `xs [--db <DB_PATH>] clone --root <ROOT_ID> --dest <RELATIVE_PATH> [--my-org <ORG>]... <REMOTE>` 只写入所选注册 root 的新相对目录；global `--db` 可置于 `clone` 前后，remote 凭据走 Git credential helper/SSH agent，不得写进 URL 或错误日志。

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

The current source includes `mise run release:prepare`: dry-run minor by default, explicit patch while pre-1.0, and explicit major only when the workspace is already 1.0+. Non-first bumps require maintainer-installed `cargo-release` 0.25.20 on PATH; first-release `0.1.0 --first-release` verifies the version and no-ops without requiring `cargo-release`. The workflow does not install the tool automatically. Moving from 0.x to 1.0 requires a separate, explicitly approved stability PLAN and a one-time manual workspace version change; it is not a routine automated bump.

## Staging 与 E2E（真实数据手册）

- **当前工作树的 CLI 数据选择实现**：source build 缺省 dev，默认 `.staging/dev-001/xingshu-dev.db`；CLI 使用独立 resolver；`--db` > `XINGSHU_DB` > build/environment default。prod 指向 `.staging/` 时拒绝。
- **v0.1.0 CLI 路径行为（源代码实现）**：官方 release CLI build 设置 `XINGSHU_CLI_BUILD_CHANNEL=release`，无 `XINGSHU_ENV` 时默认 prod；release CLI 数据库位于 OS user-data，dev 库位于 data root 的 `dev/` 子目录；源码 CLI dev 仍用 `.staging`，源码 CLI 显式 prod 用 OS user-data。`XINGSHU_ENV` 仅接受 `dev|prod`，非法值报错。Server 沿用既有 resolver：dev 默认 `.staging/dev-001/xingshu-dev.db`，prod 默认 CWD `xingshu-prod.db`，启动日志输出实际路径。首发 schema 为 v5，不导入 `.staging`；v0.1.0 用户资产仅 Windows/Linux x86_64；macOS 为实验性构建，不在该版本支持矩阵。正式二进制和当前支持平台以 XS GitHub Releases 页面为准。
- XS CLI 的 Git 操作要求系统已安装 Git，且 `git` 可从 PATH 调用。
- 真实数据基线只允许只读参考仓；`real-mini-001` 与 `real-edge-001` 均为 `.staging/` 合成副本（已 gitignore），双 DB 是回归基线，不属于 release 初始数据。
- 重建：`pwsh scripts/create-real-test-staging.ps1 [-WithEdgeCases]`（底层走 `stage-sample.ps1 -Repository @(...) -Execute`，拷贝后校验 `.git` 与文件数，再 `roots add` + `scan`）。
- 手动测试：缺省即进 dev 库，直接 `cargo run -p xingshu-server`（12681）+ Vite `pnpm --ignore-workspace run dev`（host/port 以 `webui/vite.config.ts` 为准：`127.0.0.1:12680`）；浏览器打开 `http://127.0.0.1:12680`。换 run 才设 `$env:XINGSHU_DB`；server 生产库用 `mise run prod:server`，该 server 路径契约与 release CLI 分开。

## E2E 排障三件套

- `validate-plan.py` 路径：`uv run .agents/skills/plan-mode/scripts/validate-plan.py plans/PLAN-XXX.md`（workspace 根执行）。
- `playwright-cli` 直接调用（禁 `npx` 前缀，防 npm 初始化延迟与 `Unknown project config` 噪音）；每条命令带 bash timeout。
- `e2e/playwright.config.ts` 的 `webServer.cwd` 必须指向项目根，否则 `ServeDir webui/dist` 相对路径 404；`webServer` 会重建 `.staging/e2e-playwright` 合成 5 仓。
- **git 组合参数拆分**（PLAN-250 实测）：`run_git` 的 args 数组中，带空格的组合 revspec（如 `"HEAD ^<base>"`）会被 git 当作单个无效参数静默失败（`rev-list --count` 返回 0）——必须拆成独立元素 `&["rev-list", "--count", from, &format!("^{base}")]`。
- **git cherry 方向**：`git cherry <upstream> <head>` 列出 head 侧提交，`-` 行为 patch 等价（上游已有同内容提交）；配对本地/上游提交用 patch-id 反查，不要假设方向。

## Structure

- `crates/xingshu-core/`: SQLite、scanner、Git backend、policy、puller、mover
- `crates/xingshu-cli/`: CLI executable `xs` (PLAN-0587/0617; `clone/index/search/files/grep/read` available; no `xingshu` binary alias)
- `crates/xingshu-server/`: localhost Axum API and static WebUI host
- `webui/`: Vue/Vite + Tailwind v4 + shadcn-vue UI（组件 vendor 于 `src/components/ui/`，主题 token 见 `src/shadcn-theme.css`；新依赖用 `pnpm --ignore-workspace add <pkg>@<version>` 并过 7 天冷却与构建脚本评审；视图在 `src/views/`、业务面板在 `src/components/`、纯函数与请求封装在 `src/lib/`，`App.vue` 仅做布局与状态编排）
- `e2e/`: Playwright CLI E2E（`playwright.config.ts` + `tests/xingshu.spec.ts` 19 用例 + `scripts/start-e2e-server.ps1`；含 dirty/diverged 冲突面板、只读 diff、标签策略来源、搬迁空间预检、open API、favicon 用例）
- `scripts/create-test-staging.ps1`: only synthetic staging generator
- `scripts/create-real-test-staging.ps1`: real-git staging helper（从经授权的只读参考仓生成 `.staging/` 副本）
- `scripts/stage-sample.ps1`: guarded copy helper, dry-run by default
- `docs/`: project documentation and API contract

## Git Remotes

- `origin`: private development repository
- `public`: filtered public release repository

Do not push credentials, staging data, runtime databases, `target/`, or `webui/node_modules/`.

## Backup & Conflict

- `third-party`/`third-party-frozen` 为只读，`modify_lock=1`；`fork/own` 可写。`repo_kind` 首次启发后重扫不覆盖，需 `kind set` 显式改。
- 冲突三态（PLAN-250）：`dirty`（工作区变更，面板含 status 明细+后果对照）、`diverged/non-ff`（fetch 后分叉，双栏提交清单+`git cherry` ≡ 补丁等价标记）、`ahead_clean`（纯领先不进面板，`pull` 记 `ahead`，列表显示"本地领先"徽标）。离线回退旧判定。
- 拉取冲突（工作区脏或非 fast-forward）策略：`Stop` 直给 `ConflictNeedsDecision`（携带 `ConflictInfo`），`Skip`（原 `Abort`，值 `abort` 兼容）跳过，`Backup` 走 `fs::rename → .bak.<YYYYMMDDHHmmSS.mmm>`（同父目录、原子移动、已存在则 `_1`/`_2` 递增，`scanner` 通过 `*.bak.*`/`*.broken.*` 忽略；重克隆失败自动回滚并把 `.bak` 改回原名，写 `backup_failed` 审计），`Overwrite` 走 `reset --hard + clean -fd + pull --ff-only`（UI 强制二次确认）。
- `POST /api/v1/open`：打开仓库/备份目录（canonicalize + 已注册 root 白名单，防 CSRF 越界）。
- `GET /api/v1/repos/{id}/diff?path=`：单文件只读 diff（相对 HEAD，512 KiB 字符边界截断；越界/绝对路径 400、缺失 404、bare 拒绝；未跟踪/二进制/unborn 以 `kind` 标记不伪造 patch；不写日志不落盘）。冲突面板 dirty 明细行可点击弹出。
- `GET /api/v1/roots/{id}/space`：只读磁盘可用空间（未知 root 404；探测失败 502 `space_probe_failed`）。抽屉搬迁前预检，`available <= size`（含相等）禁用搬迁。
- `GET /api/v1/repos/{id}/policy` 返回生效层：`source`（repo/tag/kind-default）+ `sourceTagId` + `effective`；标签策略是继承默认层（仓级优先，多标签 first-match）。
- 备份后拉取为**全量重克隆** `git clone <remote_url> <name>`，非增量 `fetch`；无 `remote_url` 时拒绝克隆并保留备份。

## Output Readability

- `webui/src/components/TaskPanel.vue` 不直接渲染 `task.resultJson` 原串；`parsedTaskResult / taskSummaryText`（TaskPanel 内）与 `labelResult / labelStatus / formatDuration`（`src/lib/format.ts`）解析并中文标签化（成功/已中止/已跳过/失败/冲突需决策），胶囊徽章+表格+扫描统计，`Failed to fetch` 映射为中文服务检查提示。详见 `docs/i18n/zh-Hans/DEV-006-backup-output.md` 与根 skill `readable-output`。
