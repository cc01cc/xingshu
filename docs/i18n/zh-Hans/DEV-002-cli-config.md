# CLI 参考与配置

## 安装与端口

首选 `mise run setup`（`mise.toml`）安装开发依赖；从 `webui/` 单独安装仍必须使用 `pnpm --ignore-workspace install`。WebUI dev 端口为 `12680`，Axum API 和构建后 WebUI 端口为 `12681`，一键启动见 `mise run dev`。

## CLI v0.1.0 安装与发布流程

`v0.1.0` 是首个稳定版本号。候选快照 push 到公开 XS 仓 `release-candidate/v0.1.0` 分支后触发 push-only 候选 workflow；没有 `workflow_dispatch` 候选入口。版本保持 `0.1.0`、不创建 RC tag。Windows artifact 名为 `xingshu-candidate-x86_64-pc-windows-msvc-attempt-<run_attempt>`，Linux artifact 名为 `xingshu-candidate-x86_64-unknown-linux-gnu-attempt-<run_attempt>`，各保留 90 天且每次 attempt 不可变；每个与同一 `github.sha`、`candidateRunAttempt` 关联，内含对应归档、匹配归档的 `.sha256` 文件及 `xingshu-v0.1.0-<target>.manifest.json`；manifest 记录 `candidateRunAttempt`、`minimumOsAbi`、`buildRunner`、`crtLinkage` 和 `runtimeDependencies`。候选 artifact 只用于验证，**不是 GitHub Release，也不是稳定版**；候选只含每归档 `.sha256`，不含汇总校验文件。正式 GitHub Release 另提供汇总文件 `SHA256SUMS`。正式 CLI 二进制和当前可用版本以 XS GitHub Releases 页面为准。

首发候选 artifact 与稳定 Release 只包含 Windows x86_64、Linux x86_64。Windows release target 静态链接 MSVC CRT；候选 CI 执行 `dumpbin /dependents`，若仍导入 VCRUNTIME/MSVCP/UCRT 则失败。manifest 的 `minimumOsAbi` / `buildRunner` 分别为 Windows `Windows 10+` / `windows-2025`、Linux x86_64 GNU `glibc 2.35+` / `ubuntu-22.04`；`crtLinkage` 记录 `static`，`runtimeDependencies` 记录 Windows 10+ 与 PATH 中的 Git，或 Linux glibc 2.35+、PATH 中的 Git 与 `df`（GNU coreutils，用于移动前剩余空间检查）；Linux manifest 另列 runner 上 `ldd` 的动态库 SONAME，并在依赖缺失时阻断构建。tag workflow 按 target allowlist 校验这些字段；候选 run 提供实际安装/运行证据后，平台下限才算验收。PE import 门禁通过后，Windows 不需要 VC++ Redistributable。Linux arm64 与 macOS 不属于 `v0.1.0` 用户支持目标；稳定 Release 页面是实际可用资产和支持平台的权威清单。稳定归档名为 Windows `xingshu-v0.1.0-x86_64-pc-windows-msvc.zip`、Linux `xingshu-v0.1.0-x86_64-unknown-linux-gnu.tar.gz`。

签名与完整性：候选和稳定 manifest 的 `signatureStatus` 均为 `unsigned`，平台归档不做代码签名。Windows 可能显示 SmartScreen 或“未知发布者”警告；Linux 归档同样未签名。归档旁 `.sha256` 和 Release 的 `SHA256SUMS` 仅用于检查文件完整性，不证明发布者身份。

安装步骤（候选与稳定资产都按对应 target 选择）：

1. 从对应 Actions 运行记录下载候选 artifact，或从公开 Releases 下载稳定归档；同时下载与归档同名追加 `.sha256` 的校验文件和对应 manifest（例如 `xingshu-v0.1.0-x86_64-pc-windows-msvc.zip.sha256`）。稳定 Release 另含汇总 `SHA256SUMS`；单个归档直接使用其 `.sha256` 文件校验。候选 artifact 和稳定 Release 的归档/单文件校验名称一致。
2. 校验归档哈希：Windows 使用 `Get-FileHash <archive> -Algorithm SHA256`；Linux 使用 `sha256sum <archive>`。值必须与该归档对应的 `.sha256` 文件一致。macOS 当前没有官方用户归档，不适用此安装步骤。
3. 解压并将 `xs`（Windows 为 `xs.exe`）放进用户可写且位于 PATH 的目录；打开新终端运行 `xs --version`、`xs db path` 和 `git --version`。
4. Git 是仓库操作命令的运行时依赖。当前 CLI 会在打开数据库前检查 Git；若 `git` 不在 PATH，会报 ``git command failed: Git executable not found on PATH; install Git and ensure `git` is available, then restart the terminal``。安装 Git 并重开终端后重试。`xs db path` 是例外，不依赖 Git。候选包不用于日常数据；验证时使用隔离的临时用户 profile。

稳定版安装/升级资产只在候选验收并获最终用户批准后提供；release owner 将候选同一 commit fast-forward 到公开 `main` 并 push `v0.1.0` tag 触发稳定发布。卸载时移除 CLI 程序即可，用户数据目录不会随程序一并自动删除。

## 版本与发布

- 首个稳定版本号为 `0.1.0`。候选 artifact 使用 `0.1.0` 版本号但不是公开 Release；稳定 tag `v0.1.0` 由最终批准后的 tag-push 流程创建。
- `1.0.0` 前都属于不稳定版本：0.x 默认 bump 为 minor；维护者可以显式选 patch，仅用于不改变公开行为/配置/数据语义的修复。Agent 不能自动将默认 minor 降为 patch。
- 0.x minor 用于新功能和破坏性 CLI/配置/默认路径/schema/data 变化；这类变化必须在 CHANGELOG 标记 `BREAKING` 并说明影响。0.x 时 `release:prepare` 拒绝 major；1.0+ 按标准 SemVer，minor 默认，维护者可显式选 patch 或 major。0.x→1.0 是单独批准的稳定性决策/PLAN，要求一次性手动修改 workspace 版本；不是常规自动 bump，当前命令没有 `--version` 参数。
- 稳定发布仅由公开仓稳定 `v*` tag push 触发，tag 必须是无 prerelease 后缀的稳定 SemVer。候选验收并获最终批准后，release owner 将同一 commit fast-forward 到公开 `main` 并 push `v0.1.0` tag。workflow 从公开 `release-candidate/v0.1.0` 找到同一 source SHA、`run_attempt` 的成功候选 run，下载该 attempt 的不可变 artifact，按 target allowlist 核对 manifest 的 `minimumOsAbi`/`buildRunner`/`crtLinkage`/`runtimeDependencies` 字段及哈希后用原始资产创建 Release，不重建、不接收 run ID 输入、不 push 私有仓。
- 当前源码已包含 `mise run release:prepare`：默认 dry-run + minor；首发传 `--first-release` 时先验证 workspace 为 `0.1.0`，再 no-op，不需要 `cargo-release`。非首发 bump 要求维护者手动安装 `cargo-release` 0.25.20 并加入 PATH；流程不会自动安装。0.x 可显式选择 patch，不允许 major；workspace 达到 1.0+ 后，按标准 SemVer 可显式选择 patch/major。只有 `--execute` 才写版本。该任务只调用 Cargo workspace version step 更新版本/lockfile，不发布 crate、不打 tag、不 push；CHANGELOG 由维护者按实际变更审核整理。0.x→1.0 需单独批准的稳定性 PLAN 和一次性手动 workspace version change；命令未提供 `--version` 参数。Agent 不能自动把默认 minor 降为 patch。
- 候选 Actions artifacts 只用于验证，不作为稳定安装包；正式 CLI 二进制及当前可用版本以 XS GitHub Releases 页面为准。

## 数据库选择

**当前源码构建与 server 行为**：`xingshu-<env>.db`；dev 默认 `.staging/dev-001/xingshu-dev.db`，server 设置 `XINGSHU_ENV=prod` 时默认 `./xingshu-prod.db`。当前源码 CLI 的优先级为显式 `--db` > `XINGSHU_DB` > 默认值；prod 指向 `.staging/` 的路径会被拒绝。

**v0.1.0 官方 CLI 路径行为（当前源代码实现）**：官方 release CLI 默认使用 OS 当前用户数据目录；源码 checkout CLI 的 dev 默认仍为 `.staging/dev-001/xingshu-dev.db`。`XINGSHU_ENV=dev|prod` 显式选择 CLI 运行环境，发行版的 dev/prod DB 都位于 OS 用户数据目录，非法值报错。最终优先级为显式 `--db` > `XINGSHU_DB` > build marker / 环境默认路径。首次正式运行创建 schema v5，不导入开发 `.staging` 库；FTS 搜索 sidecar 属可重建缓存。正式 CLI 二进制及当前平台支持状态以 XS GitHub Releases 页面为准。

server 保持独立的现有路径契约：dev 默认 `.staging/dev-001/xingshu-dev.db`，`XINGSHU_ENV=prod` 默认 `./xingshu-prod.db`，启动首行日志打印实际 DB 路径。CLI 发行版的用户目录规则不改变 `mise run dev:server` / `mise run prod:server`。

`--db` 是全局 CLI 参数（clap global），可写在子命令前后任意位置；绝对路径按指定路径使用，相对路径按当前工作目录解析。

## CLI 数据库备份与恢复

上面的 `xs backup` 命令备份 Git 仓库，不备份 CLI 数据库。先用 `xs db path` 获取实际生效的数据库路径。官方发行版默认路径：Windows `%LOCALAPPDATA%/Xingshu/xingshu-prod.db`，dev 库 `%LOCALAPPDATA%/Xingshu/dev/xingshu-dev.db`；Linux prod 库 `${XDG_DATA_HOME:-~/.local/share}/xingshu/xingshu-prod.db`，dev 库在相同数据根下的 `dev/xingshu-dev.db`。源码 CLI 的 dev 库仍在 checkout `.staging`。macOS 不在 v0.1.0 用户支持矩阵中。

冷备份前停止所有可能写入该数据库的 CLI 命令和 server 进程。将 `.db` 文件及同名的 `-wal`、`-shm` sidecar（若存在）作为一组复制，保持它们的主文件名一致。FTS 搜索 sidecar 是可重建的派生数据，不必备份；如缺失或过期，可在恢复库上运行 `xs --db <RESTORED_DB> index rebuild`。

恢复时先把整组文件放到隔离 profile 或备用数据库路径。用 `xs --db <RESTORED_DB> db path` 确认目标，再运行 `xs --db <RESTORED_DB> roots list` 和 `xs --db <RESTORED_DB> stats` 验证数据；通过后再切换到活动路径，切换前停止所有写入进程。卸载 CLI 只移除程序，用户数据仍保留；删除用户数据是另一个需要明确执行的操作。

`TAGS` 是星枢自己的领域标签，不是 Git tag。`repo_kind` 区分第三方、冻结、fork 和自有仓库；`policy set` 可设置 repo 或 tag 的更新和冲突策略，repo policy 优先。

## 日志与输出契约

日志（完整变量见 DEV-004）：

- 级别：`XINGSHU_LOG_LEVEL`（回退 `RUST_LOG`），默认 `info`；设为 `off` 可完全关闭。
- 载体：默认 JSONL 输出到 **stderr**；设置 `XINGSHU_LOG_FILE` 后改为按大小轮转的 JSONL 文件日志。
- stdout 只承载业务输出；`scan` 的报告是纯 JSON，可直接管道给 `jq`/脚本解析。

退出码：

- `0`：命令成功。
- `1`：参数或领域错误；stderr 输出 `Error: <原因>`（如路径不存在、repo/tag 不存在、策略非法）。

例外：`pull` 的单仓失败（网络错误、无 upstream、冲突 `stop` 等）只写入 stderr 与 `fetch_log`，**进程退出码仍为 0**；脚本编排不能只依赖退出码，应检查输出或 `fetch_log`。

各命令输出契约：

| 命令 | stdout | stderr |
|------|--------|--------|
| `roots add` | `registered root <id>` | 日志 |
| `roots list` | root JSON 数组（pretty） | 日志 |
| `roots rm` | 无 | 日志 |
| `db path` | 有效环境与绝对 `dbPath` JSON | 错误信息（如路径环境变量无效） |
| `clone` | `cloned <org>/<name> into root <id> at <rel_path> (HEAD <sha>)` | 固定安全错误摘要 + 日志；不透传 Git stderr |
| `scan` | ScanReport JSON（pretty） | 逐仓进度 + 日志 |
| `list` | RepoRecord JSON 数组（pretty），空结果 `[]` | 日志 |
| `tag` / `untag` | 无 | 日志 |
| `kind set` | 无 | 日志 |
| `policy set` | 无 | 日志 |
| `pull` | 每仓一行 `[i/total] org/name: result (ms)` | `pull started/finished` 汇总 + 失败明细 + 日志 |
| `move` | `moved to <path>` | 日志 |
| `stats` | `repositories: N` / `bytes: N` 两行 | 日志 |

## 命令参考

### db path

```text
xs db path
```

- 只读输出 JSON：`environment` 是当前有效环境，`dbPath` 是最终解析后的绝对路径。
- 在打开 SQLite 数据库或创建其父目录前运行；不要求系统 Git 存在，也不创建数据库文件。

### roots

```text
xs roots add <PATH> [--disk LABEL] [--priority N]
xs roots list
xs roots rm <ID>
```

- `add`：注册 root 目录；`PATH` 必须已存在且为目录，否则退出 1。`--disk` 记录磁盘标签，`--priority` 记录优先级（默认 0）。
- `list`：输出全部 root 字段（`id/name/path/disk_label/mount_point/priority/created_at/updated_at`）。
- `rm`：按 `ID` 移除登记；`ID` 不存在时静默成功（幂等）。只影响索引，不触碰磁盘上的仓库。

### clone

```text
xs [--db <DB_PATH>] clone --root <ROOT_ID> --dest <RELATIVE_PATH> [--my-org <ORG>]... <REMOTE>
```

- `REMOTE` 支持 HTTP(S)、`ssh://`、SCP-like 和 `file://` Git remote；不支持裸相对路径或 Windows drive path。HTTP(S)/file URL 不得带 userinfo；所有形式不接受 query/fragment。SSH/SCP-like 只允许无用户名或字面 `git` 用户名。私有仓库通过系统 Git credential helper 或 SSH agent 认证；不要把密码/token 放进 URL。
- `--root` 必须是当前 `--db`/`XINGSHU_DB` 对应数据库中的已注册 root id；常规 CLI DB open/migration 仍在命令 dispatch 前发生。`--dest` 是 root 内相对路径，父目录必须已存在；绝对路径、盘符/UNC、`..`、destination parent 中任意 symlink/junction、已存在目录/文件、stale repo row，以及落入另一个已注册 root 的目标均拒绝。目标目录由 Core 独占创建。
- 可重复 `--my-org` 的分类语义与 `scan --my-org` 相同；省略时按空 owner 列表分类：存在 `upstream` remote 为 `fork`，否则默认为 `third-party`。
- 成功时只登记该仓库的元数据，不扫描整个 root，也不自动全文索引；需按需执行 `xs index update --repo <ORG/REPO> --root <ROOT_ID>`。
- stdout 输出 repo key、root id、相对目标路径和 HEAD。失败返回非零；Git stderr 不原样显示。Git spawn 后失败或 clone 得到无 HEAD 仓库时不自动清理，错误提示目标是否仍存在；DB 登记失败时保留已克隆文件。

### scan

```text
xs scan [--root PATH]... [--max-depth N] [--include-nested] [--my-org ORG]...
```

- `--root` 可重复；省略时扫描全部已注册 root。传入未注册路径 → 退出 1。
- `--max-depth`：`0`（默认）表示不限层级。
- `--include-nested`：索引嵌套仓库（默认只索引外层仓库）。
- `--my-org` 可重复：remote owner 命中时首次分类为 `own`；仓库存在指向别处的 upstream 时优先判定 `fork`。
- stdout 为扫描报告：`roots_scanned / repos_found / broken_repos / skipped_directories / nested_repos / errors`。
- stderr 逐仓进度：`[N] <rel_path>: <clone_status>`。

### list

```text
xs list [QUERY] [--tag TAG] [--root ID] [--status STATUS] [--org ORG]
```

- `QUERY` 大小写不敏感，匹配 `org/name/rel_path`。
- `--tag` 为星枢领域标签（非 Git tag）；多个过滤条件为 AND 语义。
- 输出 RepoRecord 数组（pretty JSON）；空结果输出 `[]` 并退出 0。

### tag / untag

```text
xs tag <repo> <tag...>
xs untag <repo> <tag...>
```

- `<repo>` 支持 `name`、`org/name` 或 rel_path（如 `local/demo`）。
- `tag`：标签不存在时自动创建；`untag`：标签不存在 → 退出 1（`Error: tag not found: ...`）。
- 成功静默（无输出，退出 0）。

### kind set

```text
xs kind set <repo> <third-party|third-party-frozen|fork|own> [--upstream REMOTE]
```

- 显式设置 `repo_kind`（重扫不会覆盖）；`--upstream` 记录 fork 的上游 remote。
- 非法 kind → 退出 1（`Error: invalid repository kind: ...`）。

### policy set

```text
xs policy set [REPO] [--tag TAG] [--strategy fetch-only|mirror|archive|no-update] [--conflict stop|backup|overwrite|skip] [--unattended stop|backup|overwrite|skip]
```

- `REPO` 与 `--tag` 二选一；同时给出或都未给出 → 退出 1。
- 默认值：`--strategy fetch-only`、`--conflict stop`、`--unattended skip`。
- repo 级策略优先于 tag 级策略。

### pull

```text
xs pull [--jobs N] [--unattended]
```

- 默认交互态逐仓串行；`--unattended` 按 `--jobs`（默认 8）分块并发。
- 冲突处理：交互态读 repo/tag 的 `pull_conflict_policy`（默认 `stop` → 冲突仓以错误行输出到 stderr 并跳过，不自动改写）；unattended 读 `unattended_conflict_policy`（默认 `skip`）。
- 成功、跳过与 `skip` 结果写入 `fetch_log` 并更新仓库拉取状态；冲突 `stop` 不写 `fetch_log`。旧值 `abort` 仍被接受为别名（兼容历史数据与脚本）。
- 失败明细与起止汇总在 stderr；结果行在 stdout。

示例：`org/demo` 存在本地改动（dirty）触发 `stop`，`org/clean` 正常更新。

```text
# stdout
[1/2] org/clean: ok (1178ms)

# stderr（JSONL 日志行已省略）
pull started: 0/2
[2/2] <repo-root>/org/demo: repository conflict requires a user decision
pull finished: 2 repositories processed

# 退出码 0：demo 被跳过并保持原状、不写 fetch_log；clean 已更新并记录。
```

同一场景改用 `--unattended`（默认 `skip`）：

```text
# stdout（分块并发，完成顺序不定，[i/total] 为任务序号）
[2/2] org/demo: skipped (355ms)
[1/2] org/clean: ok (1224ms)

# stderr（JSONL 日志行已省略）
pull started: 0/2
pull finished: 2 repositories processed

# 退出码 0：skip 写入 fetch_log（result=skipped），demo 保持原状。
```

### move

```text
xs move <repo> <ROOT_ID>
```

- 跨 root 迁移仓库并更新索引位置；同 root 迁移或目标不存在 → 退出 1。
- 成功后 stdout 输出 `moved to <path>`。

### stats

```text
xs stats
```

- 输出 `repositories: N` 与 `bytes: 总字节` 两行。
- 目前不含按 root/tag/org 的聚合（HTTP API `/api/v1/stats` 提供 `byKind`）。

## 常用流程

```powershell
.\target\release\xs.exe --db .\.staging\run-001\xingshu-dev.db roots add .\.staging\run-001\root-a
.\target\release\xs.exe --db .\.staging\run-001\xingshu-dev.db scan --my-org cc01cc
.\target\release\xs.exe --db .\.staging\run-001\xingshu-dev.db tag network/third-party-demo network
.\target\release\xs.exe --db .\.staging\run-001\xingshu-dev.db list --tag network
```

## 安全参数

- 交互 pull 冲突策略默认 `stop`。
- unattended pull 冲突策略默认 `skip`。
- `scan_nested_repos` 默认关闭，`scan_max_depth=0` 表示不限制层级，`scan_skip_dirs` 用于剪枝。
- `third-party`/`third-party-frozen` 在星枢自身操作中受 `modify_lock` 约束；真实参考仓库只允许只读 `scan`/`list`/`stats`，禁止 pull/move/backup/overwrite。

## PLAN-0587：内容索引与搜索

**已在此版本实现。** v1 唯一 CLI 可执行入口为 `xs`，不提供 `xingshu` alias。现有 Cargo package 调用（如 `cargo run -p xingshu-cli`）和 server 名称保持不变。

```text
xs index status [--repo <ORG/REPO>]... [--root <ID>] [--json]
xs index update [--repo <ORG/REPO>] [--root <ID>] [--all-roots] [--json]
xs index rebuild [--repo <ORG/REPO>] [--root <ID>] [--all-roots] [--json]
xs search <PATTERN> [--repo <ORG/REPO>]... [--root <ID>] [--tag <TAG>] [--type <TYPE>]... [--glob <GLOB>]... [--fixed-strings] [--ignore-case] [--word-regexp] [--context <N>] [--files-with-matches] [--count] [--max-count <N>] [--max-results <N>] [--json]
xs grep <PATTERN> [--repo <ORG/REPO>]... [--root <ID>] [--tag <TAG>] [--type <TYPE>]... [--glob <GLOB>]... [--fixed-strings] [--ignore-case] [--word-regexp] [--context <N>] [--files-with-matches] [--count] [--max-count <N>] [--max-results <N>] [--ref <REV>] [--all-roots] [--json]
xs files <GLOB> [--repo <ORG/REPO>]... [--root <ID>] [--tag <TAG>] [--type <TYPE>]... [--max-results <N>] [--json]
xs read <ORG/REPO> <PATH> [--root <ID>] [--ref <REV>] [--start-line <N>] [--max-lines <N>] [--json]
```

- `index status` 只读报告 HEAD、已索引 commit、覆盖与新鲜度，不触发扫描。`index update` 增量更新所选仓库；`index rebuild` 重建派生索引。update/rebuild 必须显式选择范围（`--repo`、`--root` 或 `--all-roots`）；`--all-roots` 不得与 repo/root selector 并用。
- v1 选择 SQLite FTS5 trigram `detail=none` 内容索引。长度至少 3 的安全 literal 可用 FTS5 `LIKE` 预筛选，再精确匹配；短 literal、含 wildcard 的固定字符串与复杂 regex 只扫描缓存的已索引正文，不扫描工作树。`search` 只读取已索引 HEAD，不自动更新缺失或过期索引；缺索引/HEAD 不匹配分别报告 `index_missing`/`index_stale`，并提示执行 `index update`。
- `grep` 绕过持久索引，直接读取所选工作树中的合规文本文件，包含已跟踪修改和非忽略的 untracked 文件。每条结果标明 `source=worktree`、扫描开始时的 `baseHead` 和 `fileState`；修改或 untracked 文件没有 commit revision。使用 `--ref` 时，按本地已有 branch/tag/commit tree 搜索，不 fetch、不 checkout，来源为 `git-ref`。`--ref` 必须与恰好一个 `--repo` 同用，且不能与 `--all-roots` 并用。
- `files` 在已索引 HEAD 的路径目录中搜索 repo 相对路径（目录包含 binary 路径）；不读正文，也不扫描当前工作树。`read` 从 HEAD 或一个本地 ref 读取有界文本：默认最多 200 行，最大 5,000 行，文件上限 5 MiB。
- Repo key 格式为 `org/name`。多结果查询可聚合多个所选 root 中同名仓库，并在每条结果标注 `rootId`。`read`、index update/rebuild 等单目标操作遇到重复 repo selector 时必须用 `--root` 消歧，否则返回 `ambiguous_repo`。`--all-roots` 是显式范围，不能与 `--repo`/`--root` 并用；仅按对应契约用于 grep 与索引维护。未注册路径不会成为隐式搜索范围。
- `search`/`grep` 默认最多返回 1,000 条匹配行，`files` 默认最多返回 10,000 个路径。`--max-results` 范围为 1–100,000。达到上限时返回排序稳定的有界结果，`truncated=true`、`complete=false`、无 errors，退出码仍为 0。单行最多 4,096 个 Unicode 字符。无命中为空结果且成功；仓库级读取/扫描错误保留部分结果、设置 `complete=false`、填写 `errors` 并退出 1。
- 安全边界：这些命令不写源仓库，不 fetch、pull、checkout 或改仓库文件。索引写入仅发生于按数据库身份和环境隔离的派生缓存；重建失败保留旧的已发布 generation。默认不跟随 symlink，不搜索 `.git` 对象、binary 正文、生成目录或未注册目录。正文每文件最多 5 MiB，且必须是有效 UTF-8、无 NUL。索引只覆盖当前 HEAD，不预建历史；读取本地历史 revision 使用单仓 `grep --ref`。

## 新增命令与参数（PLAN-297）

- `scan --full`：关闭增量扫描，强制全部仓库重读元数据（默认按指纹跳过未变仓库；报告新增 `unchanged_repos`）。
- `pull --root <id>`：仅处理某个已注册 root；`--dry-run`：预演（默认零写，远端分叉标「需 fetch 检查」）；`--dry-run --fetch`：先 fetch 本地 refs 再精确预判（不写审计）。
- `stats --by root|tag|org`：按维度聚合；`tag` 存在多标签重复计入（输出已标注）。
- `status [--root <id>] [--repo <name>] [--remote]`：只读状态巡检；`--remote` 用 `git ls-remote` 检查远端是否移动（零本地写入，失败标 unknown）。
- `backup create <repo> [--dest DIR] [--archive tar.gz|tar.zst] [--keep N]`：独立备份（源仓不动，默认同父目录目录快照）；`backup list <repo>`；`backup restore <repo> <backup-path>`（恢复前当前状态转存为 `*.bak.pre-restore.*`）；`backup prune <repo> --keep N [--execute]`：清理（默认 dry-run 预览将删清单，`--execute` 才删除；与 API `dryRun` 对等）。压缩归档需系统 `tar`；创建/恢复 `.tar.zst` 还需系统 `zstd`。
- 冲突动作值统一为 `skip`（旧值 `abort` 兼容；结果值为 `skipped`）。
