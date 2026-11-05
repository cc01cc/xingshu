# 仓库目录与检索 SPEC

> 契约状态：`active`
>
> 实现状态：`partial`
>
> Profile：`product`
>
> Owner：XS 项目维护者
>
> 消费者：人类用户、Agent、检索实现者
>
> Parent：[`product-scope.md`](product-scope.md)
>
> 来源补充：PLAN-0587（CLI search/files/grep/read 命令边界，2026-10-30 用户确认）
>
> 更新：2026-10-31

## 1. 目的与边界

定义 XS 如何接纳操作者注册的仓库根目录、发现仓库、建立可检索目录、搜索代码/路径和读取内容。它细化产品总览的发现与检索能力；不定义索引引擎、数据库表、HTTP route 或具体 CLI 参数。

默认跨仓内容/路径搜索读取 XS 为注册仓生成的索引。操作者也可显式请求只读工作树搜索（grep），范围必须绑定已注册 repo/root；它只读取当前工作树文本文件，不写 repo、不 fetch/pull/checkout。该模式不替代默认索引搜索，普通查询不得隐式回退为全树扫描。

仓库根目录只能由操作者明确注册。XS 只扫描已注册 root，不预置或探测特定目录。测试和生产部署分别使用自己的 root、元数据和索引存储。

## 2. 参与者与入口

| 参与者 | 目标任务 | 入口类别 | 结果要求 |
|---|---|---|---|
| 人类用户 | 管理 roots、触发扫描、浏览/筛选仓库、搜索并读取命中内容 | WebUI、CLI | 可视化呈现 root、索引状态和读取目标 |
| Agent / 自动化 | 管理 roots、触发扫描、浏览/筛选仓库、搜索并读取命中内容 | CLI、HTTP API | 可解析返回 root、索引状态、仓库、revision、路径和匹配上下文（match context） |

人类用户和 Agent 的发现/检索功能集合相同；入口只改变交互和结果呈现，不把 roots/scan 限定给人类，也不把代码查询/内容读取限定给 Agent。

## 3. 能力状态

| 能力 | 当前状态 | 产品契约 |
|---|---|---|
| Root 注册/列举/移除与扫描 | 已实现 | 注册路径成为扫描边界；扫描不会注册或操作未登记目录 |
| 元数据目录 | 已实现 | 支持按仓库名、owner、相对路径、root、XS 标签和状态筛选 |
| 跨仓全文代码与路径检索 | 部分实现 | CLI v1 提供索引内容搜索、HEAD 路径搜索与显式 worktree/ref grep；HTTP API/WebUI 入口由 PLAN-0607 承接 |
| 单文件内容读取 | 部分实现 | CLI 可读取 HEAD 或本地 ref 的有界文本；HTTP/API/WebUI 消费由 PLAN-0607 承接 |
| 索引状态 | 部分实现 | CLI 可显示当前 HEAD、索引 commit、文件/字节覆盖与 stale 状态；Server/WebUI 对等状态由 PLAN-0607 承接 |

XS 标签由 `tags-and-policies.md` 定义，统一术语见 `README.md`；本 SPEC 只规定检索可按标签筛选。

## 4. 检索结果

每个仓库结果至少可识别其仓库名、owner、相对路径、所属 root、remote、工作状态、XS 标签和索引状态。内容命中结果须标注 source、repo、revision、路径、行位置和有限上下文。

- index 或指定 Git ref 的命中必须带精确 commit revision。
- worktree grep 必须标注 `source=worktree`、扫描开始时的 `baseHead` 和文件状态（clean/modified/untracked）；修改或未跟踪文件不得伪装成 HEAD commit 内容。

读取结果需说明读取的是哪个仓库的哪个 revision。指定 `--ref` 的读取只验证该 ref 和路径；索引陈旧只影响 index-backed 搜索，不应拒绝读取一个有效的 Git revision。用户查询仓库当前工作树 diff 与读取某一 revision 的源码是不同操作，不得混为一种内容来源。

同一 `org/name` 在多个 root 中存在副本时，search/grep/files 等多结果查询聚合所选 scope 内的匹配，并在每项结果中返回 `root_id`。read、index update/rebuild 等单目标操作若仍匹配多个副本，必须要求 root 约束或返回明确的歧义错误，不得按数据库首行任意选择。

默认索引检索只读取已登记 root 生成的索引；只有显式工作树 grep 才直接读所选已登记 repo/root。普通查询不隐式触发 grep、scan、fetch、pull、clone 或工作树修改。数据库字段、分页 token、具体排序参数和 CLI 参数留给后续实现 PLAN。

## 5. 主要流程

- 操作者注册一个或多个仓库根目录，并触发/等待扫描。
- XS 发现可识别仓库、记录元数据和异常项；嵌套仓库、损坏仓库或不可读目录有可见结果，不静默当作成功。
- 用户按元数据过滤，或跨仓搜索代码内容/路径。
- 用户从结果打开文件内容；结果继续标注 root、repo、revision 和 path，便于下一步通过生命周期能力选择仓库操作。
- root 被移除时只移除 XS 的目录登记与索引范围，不因此删除磁盘上的仓库。

## 6. 失败与安全边界

- 未注册 root、越界路径或无法确定仓库身份时，不返回其他 root 的文件内容；read 必须验证规范化目标仍在 canonical repo/root 内并拒绝越界 symlink component/target。
- 文件不存在、不可读、binary、过大、revision 无效、索引陈旧/不可用时，说明失败原因或截断状态；不把空结果伪装为“内容为空”。
- 显式 grep 只能读取明确选择的已登记 repo/root 的当前工作树文本文件；不跟随 symlink，不读取 `.git` 对象或 binary 正文，不写索引或 repo，也不触发 fetch/pull/checkout。
- 测试/生产 root 和索引分离；一个环境的扫描、重建或清理不得改写另一个环境的数据。
- index update/rebuild 先构建 repo 的临时 generation，完成后才原子发布；失败或取消保留上一完整 generation，search 不读取未完成的临时数据。
- 结果不得泄露 remote 中的凭据、PAT 或访问 token。

## 7. 验收场景

| 编号 | 场景 | 操作 | 预期结果 |
|---|---|---|---|
| D1 | 注册 root 后发现仓库 | 注册有效 root 并扫描 | 只报告该 root 内的仓库；未登记目录不被隐式扫描 |
| D2 | 跨仓代码与路径搜索 | 查询内容 pattern 或 repo 相对路径 glob，并设置 root/标签过滤 | 内容命中或文件路径结果指出 repo、root_id、revision/source、path 和位置 |
| D3 | 搜索后读取 | 从命中读取指定 revision/path | 返回对应内容；缺失、二进制、过大、不可读、revision 无效和索引陈旧情况均明确标示（结果如实报错，不记为 `success`） |
| D4 | 删除 root 登记 | 移除已登记 root | 从 XS 目录中移除该 root；磁盘仓库保持不变 |
| D5 | 隔离环境扫描 | 在测试环境触发全量/重建索引 | 生产 root、数据库、索引和仓库文件保持不变 |
| D6 | 显式工作树 grep | 指定已登记 repo/root 执行 grep | 只读当前文本工作树；结果标 `source=worktree`、`baseHead` 和文件状态；不写 repo/index、不触发 Git 网络动作 |
| D7 | 同名仓库跨 root | 两个已登记 root 含相同 `org/name` 仓库 | 多结果搜索聚合并逐项标 `root_id`；单目标操作不任意选择副本 |
| D8 | 读取路径 symlink 越界 | repo 内路径组件链接到 repo/root 外 | read 拒绝访问，不返回外部文件内容 |
| D9 | 索引更新中断 | 正在构建 repo generation 时注入失败或取消 | 上一完整 generation 保持可用，临时 generation 不进入搜索结果，状态和重试结果明确 |

## 8. 非目标与待确认

- 不选择 Zoekt、SQLite FTS 或其他索引后端；Zoekt 评估属于技术选型。
- 不定义符号搜索、语义搜索、HTTP schema、分页协议和索引文件格式。
- 更细的 index freshness 判据由实现 PLAN 定义；搜索结果必须标明索引状态与具体 revision，不能静默混合来源。
