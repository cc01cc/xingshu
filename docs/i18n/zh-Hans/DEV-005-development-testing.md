# 开发与测试

## 开发态启动（mise 首选，对齐 A03-xihe）

```powershell
mise run setup
mise run staging:dev --run-id dev-001
mise run dev          # == dev:host → 12681 server + 12680 Vite 并行，Ctrl+C 同停；server 缺省进 .staging/dev-001/xingshu-dev.db
# 换 server run：$env:XINGSHU_DB = ".staging/dev-002/xingshu-dev.db"; mise run dev
# 生产 server 库：mise run prod:server  # XINGSHU_ENV=prod，默认 CWD/xingshu-prod.db，拒 .staging/ 路径
```

数据库 ledger 当前为 schema v5。用户已确认首个正式版本前没有历史正式库/旧库；v0.1.0 CLI 在 OS 用户数据目录创建新库，不导入旧名数据库或 `.staging` 开发库。未来 schema 变更应在对应功能计划中另行定义迁移和恢复契约。

## CLI 发布流程与运行路径

- 当前 `mise run dev` / `prod:server` 始终使用 server 现有 CWD/staging 路径契约。CLI 专属数据路径解析由源代码实现；官方稳定二进制及平台可用性以 XS GitHub Releases 页面为准。
- 官方 CLI release binary 默认使用 OS 用户数据根（Windows `%LOCALAPPDATA%/Xingshu`，Linux `${XDG_DATA_HOME:-~/.local/share}/xingshu`）；源码 CLI dev 仍使用 checkout `.staging`。CLI resolver 与 server 分离。macOS 数据路径仅供实验构建使用，macOS 不属于 v0.1.0 用户安装/支持矩阵；正式支持平台和当前二进制由 GitHub Releases 页面列出。
- 首发候选快照 push 到公开 `release-candidate/v0.1.0` 分支触发 push-only workflow；候选 artifact 只面向 Windows/Linux x86_64，名称含 `-attempt-<run_attempt>` 且每次运行不可变；候选不是公开 Release。候选验收并获最终批准后，release owner 将同一 commit fast-forward 到公开 `main` 并 push 无 prerelease 后缀的稳定 SemVer `v0.1.0` tag；tag workflow 从 candidate 分支查找同 SHA/attempt 的成功构建，核对哈希后重用原资产，不重新构建。
- CLI 的仓库操作依赖系统 Git；用户安装稳定版时需确保 `git` 可从 PATH 调用。`xs db path` 显示有效环境及数据库路径，不创建数据库。
- 当前源码已包含 `mise run release:prepare`：默认 dry-run + minor；首发 `--first-release` 验证 workspace 为 `0.1.0` 后 no-op，不需要 `cargo-release`。非首发 bump 要求维护者手动安装 `cargo-release` 1.1.7 并加入 PATH，流程不会自动安装。安装命令：`cargo install cargo-release --version 1.1.7 --locked`（装到 `~/.cargo/bin`）。0.x 可显式选 patch、拒绝 major；workspace 达到 1.0+ 后可依标准 SemVer 显式选 patch/major，默认仍为 minor。只有 `--execute` 才更新版本/lockfile；目标为不发布 crate、不打 tag、不 push。0.x→1.0 需单独批准的稳定性决策/PLAN 和一次性手动 workspace version change；不是常规自动 bump，当前没有 `--version` 参数。CHANGELOG 由维护者复核整理；Agent 不自动降级到 patch。
- 1.0 前版本不稳定；0.x breaking CLI/config/default path/schema 变化用 minor 并在 CHANGELOG 标记 `BREAKING`。1.0 后使用标准 SemVer。

等价裸命令（双终端）：

```powershell
pwsh -NoProfile -File .\scripts\create-test-staging.ps1 -TargetRoot .\.staging\dev-<run-id>
# 缺省即进 .\.staging\dev-<run-id>\xingshu-dev.db，无需设变量；换 run 才设：
$env:XINGSHU_DB = ".staging\dev-<run-id>\xingshu-dev.db"

# 终端 1：Rust API
$env:XINGSHU_PORT = "12681"
cargo run -p xingshu-server

# 终端 2：Vite WebUI（host/port 以 webui/vite.config.ts 为准：127.0.0.1:12680）
cd webui
pnpm --ignore-workspace run dev
```

Vite 使用 `12680`，并将 `/api` 与 `/health` 代理至 `12681`；server 在构建完成后也可直接托管 `webui/dist`。在设置页添加根目录后，点击 **扫描索引** 再查看仓库列表。

## 基础验证命令

```powershell
mise run validate     # lint + typecheck + build + test
# 细粒度
mise run lint        # clippy + fmt --check
mise run typecheck   # vue-tsc
mise run build
mise run test
# 裸命令
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace --release
cd webui
pnpm --ignore-workspace install
pnpm run typecheck
pnpm run build
```

更多入口见根 `mise.toml`：`dev:server/dev:ui/setup/clean/staging`。

## 测试分层

- Unit：分类、脱敏、policy、路径和纯函数。
- Integration：临时 Git 仓、bare remote、SQLite schema 和 migration。
- Real-process：编译后的 CLI/server 与 staging 数据。
- Playwright MCP：真实 server/Vite、真实 API、桌面和移动 viewport。

任务队列额外覆盖：`POST /api/v1/tasks` 返回 `202/taskId`，GET 返回持久化终态，SSE 返回 snapshot 和终态事件；任务级冲突覆盖 `waiting_for_decision`、conflicts 查询、backup/overwrite/abort 和重复决策 `409`。独立 real-process 测试使用合成 tempdir 数据启动编译后的 server，验证 HTTP 与 SSE 全链路。WebUI 的 `task-store.ts` 覆盖 EventSource、GET 快照/冲突回退和指数退避重连。

所有写操作测试使用新的 `.staging/<run-id>` 或 tempdir。真实参考仓库不执行 pull、move、backup、overwrite。
