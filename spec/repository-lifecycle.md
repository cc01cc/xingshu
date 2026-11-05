# 仓库生命周期与操作 SPEC

> 契约状态：`active`
>
> 实现状态：`partial`
>
> Profile：`product`
>
> Owner：XS 项目维护者
>
> 消费者：人类用户、Agent、仓库操作服务
>
> Parent：[`product-scope.md`](product-scope.md)
>
> 关联：[`tags-and-policies.md`](tags-and-policies.md)、[`backup-lifecycle.md`](backup-lifecycle.md)

## 1. 目的与边界

定义已注册仓库的生命周期操作：kind/状态查看、clone、pull/update、跨 root move、冲突决策和逻辑归档/恢复。根目录注册归 [`repository-discovery.md`](repository-discovery.md)；XS 标签和有效策略由 [`tags-and-policies.md`](tags-and-policies.md) 单独定义；用户主动备份由 [`backup-lifecycle.md`](backup-lifecycle.md) 定义。

本 SPEC 定义操作意图、目标和可见结果，不冻结 Git 参数、HTTP route、CLI 参数或实现策略。用户未登记的目录不是隐式操作目标。

| 能力 | 当前状态 | 契约方向 |
|---|---|---|
| 查看状态、pull/update、move、冲突处理 | 已实现 | 保留并明确目标、策略来源、结果和失败状态 |
| Clone 到显式目标 root | 部分实现（CLI only，PLAN-0617） | CLI 运行时验收已通过（PLAN-0617 V8）；API/WebUI caller 后续承接 |
| 仓库逻辑归档/恢复 | 未实现 | 保持后续增强提案，不与独立备份或 pull 策略值 `archive` 混淆（后者为现有 CLI/API 接受的策略取值，行为等同禁止更新；其最终语义由后续实现 PLAN 处理） |

## 2. 生命周期状态与参与者

产品至少区分已登记活动仓库、操作中/待决策、操作完成/失败，以及拟新增的逻辑归档仓库。logical archive 是提案状态，不代表当前代码已实现。

| 参与者 | 主要操作 | 入口类别 | 操作确认 |
|---|---|---|---|
| 人类用户 | 查看状态、选择仓库、clone、pull、move、解决冲突、归档/恢复 | WebUI、CLI | 可视化呈现目标、策略、任务状态与操作结果 |
| Agent / 自动化 | 查看状态、选择仓库、clone、pull、move、解决冲突、归档/恢复 | CLI、HTTP API | 可解析取得同一操作集合的目标、策略、任务状态与结果；不能绕过 effective policy 或环境边界 |
| 操作服务 | 校验 root/repo/policy，执行单仓或批量动作，记录结果 | Core（UI/CLI/API 共用） | 遇到未知目标或拒绝策略时拒绝执行（fail closed） |

人类用户与 Agent 的仓库生命周期能力集合相同；确认方式由操作风险决定，不因请求来自 Agent 而移除该功能。需要人工确认时 Agent 可以发起并进入待决策状态，但不能自我确认。

## 3. 操作契约

- **Clone**：用户选定来源仓库和目标注册 root；目标已存在、路径越界或环境不匹配时拒绝，不覆盖同名目录。
- **Pull/update**：用户/Agent 选定一个已登记仓库；执行前读取其 effective policy，报告策略来源、结果和失败原因。未知/禁止策略不允许写。
- **状态/冲突**：将 dirty、ahead、behind、diverged/non-ff 等状态与用户可采取的动作对应；纯本地领先不应伪装为需要覆盖的冲突。
- **冲突处理**：stop 保持仓库不变并请求决策；skip 明确跳过；backup 在 pull 流程保存原仓库并重建，区别于用户主动创建的备份副本；overwrite 明示丢弃内容并获取确认。
- **Move**：移动到明确目标 root；执行前展示目标空间/失败前置；移动失败时报告原/目标位置和仓库可恢复状态，不默默丢失仓库。
- **逻辑归档/恢复（后续提案）**：归档使仓库退出默认活动列表和批量写操作；文件、Git 元数据和备份保留；从归档视图可恢复。它不等于删除、move、生成 tar 文件，也不是 pull 策略值 `archive`。

## 4. 操作任务和结果

每个操作结果至少能识别目标仓库/root、操作类型、开始/结束结果、`README.md` 统一状态枚举（success/skipped/pending_confirmation/failed）和可操作错误原因。批量操作不得用一个总体成功掩盖单仓失败。

后台任务期间的冲突必须让其余不冲突仓库继续处理；用户决策只影响明确列出的冲突仓库。Agent/CLI/API 能读取与 WebUI 一致的任务结果和冲突决策状态。

## 5. 安全与失败边界

- 操作目标始终是明确的已注册仓库及 root；不得仅按同名仓库隐式选择目标。
- 测试/生产 root 相互隔离；无法确认操作环境时拒绝写入。
- repo/tag/kind 的策略解析由 `tags-and-policies.md` 唯一拥有；生命周期操作只消费有效结果。
- 策略许可不等于单次人工确认：overwrite 等破坏性冲突操作需本地人类操作者确认；pull 的无人值守（unattended）策略是操作者预先设置的持续授权，不授权 Agent 自我确认。独立 backup restore/prune 遵循 `backup-lifecycle.md` 的逐次确认。
- 覆盖、本地清理、删除或归档/恢复等会影响活动状态的操作，必须显示作用范围和后果；不可推断为用户授权。
- clone/pull/move/archive 失败时标明实际状态；失败回滚不保证始终成功，若仓库位置/状态不确定必须报告人工核对要求。

## 6. 验收场景

| 编号 | 场景 | 操作 | 预期结果 |
|---|---|---|---|
| L1 | Clone 到明确 root | 选仓库并选择测试 root | 新目录只出现在测试 root；production 和其他 roots 不变；同名目标不覆盖 |
| L2 | Pull 策略拒绝 | 选择 `no-update` 仓库并请求 pull | 写操作开始前拒绝（结果记为 `skipped`，拒绝语义），并显示 effective policy 来源 |
| L3 | Dirty 冲突 stop/skip | 对 dirty 仓库执行 pull | stop 保留仓库并进入待决策；skip 明确记录跳过，其他无冲突仓库可继续 |
| L4 | Conflict backup | 用户选择 pull 冲突备份策略 | 旧仓库和新仓库结果可辨识；与用户主动 backup 的可恢复副本语义区分 |
| L5 | Overwrite | 选择会丢弃本地内容的动作 | 明示影响并取得确认；未确认则状态不变 |
| L6 | Cross-root move | 选择明确目标 root | 显示空间/目标检查结果；失败时提供可恢复状态，不误报成功 |
| L7 | Logical archive | 归档活动仓库后从归档视图恢复 | 默认活动列表/批量写操作排除归档项；仓库和备份保留；恢复回活动视图 |

## 7. 非目标与待确认

- 不定义 Git push、remote 托管或用户手动使用 Git 的行为。
- 不承诺 move/overwrite/archive 的数据库或文件系统回滚永不失败；要求结果可诊断和 recovery status 可见。
- 逻辑归档仍为后续产品提案；首期优先级由用户在 SPEC 批准时确认。
- 冲突 task API、状态 enum、Git 参数和实现级原子性留给后续技术契约/实现 PLAN。
