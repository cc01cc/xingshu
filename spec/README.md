# XS 项目 SPEC

> 契约状态：`active`
>
> 实现状态：`partial`
>
> Profile：`governance`
>
> Owner：XS 项目维护者
>
> 消费者：XS Agent、用户、后续 PLAN 与实现者
>
> 来源：PLAN-0424
>
> 更新：2026-10-31

## 1. 目录角色

`spec/` 保存 XS 独立于开发者/用户文档的目标产品契约。契约描述长期产品定位、边界和可验收行为；`proposed` 文档不得被当作当前实现事实。

本目录不是：

- PLAN 任务状态、审查记录或原始 evidence；
- 开发者教程、运行命令或公开文档站导航；
- OpenAPI、数据库 schema 或测试正文；
- 具体技术选型和实现计划。

具体 SPEC 由对应 PLAN 承接。尚未取得用户明确确认的草案保持 `proposed`，不得作为冻结实现基线。

契约状态与实现状态是两条独立轴：`proposed` 表示整份领域契约尚未获用户接受，`active` 表示需求基线已被接受。实现状态 `implemented`/`partial`/`planned` 描述代码支持度；能力状态表描述各能力当前覆盖度。一个领域文件可以列出已实现能力，但该文件的 proposed 规则和验收要求仍未批准。

## 2. 当前契约

产品范围总览与四份领域契约均为 `active`（原契约于 2026-09-30 批准冻结；repository-discovery 的 CLI 搜索边界于 2026-10-30 由用户确认补充）；实现状态为 `partial` 的能力由后续实现 PLAN 补齐。

| 契约 | 契约状态 | 实现状态 | Profile | Canonical 中文文件 | 承接关系 |
|---|---|---|---|---|---|
| 产品定位与能力范围 | active | partial | product | [product-scope.md](product-scope.md) | PLAN-0424；用户于 2026-09-29 确认冻结 |
| 仓库目录与检索 | active | partial | product | [repository-discovery.md](repository-discovery.md) | PLAN-0425；CLI search/files/grep/read 边界由 PLAN-0587 承接（2026-10-30 用户确认）；HTTP/API/WebUI 对等入口由 PLAN-0607 承接 |
| 标签与策略 | active | partial | product | [tags-and-policies.md](tags-and-policies.md) | PLAN-0425；用户于 2026-09-30 批准冻结 |
| 仓库生命周期 | active | partial | product | [repository-lifecycle.md](repository-lifecycle.md) | PLAN-0425；用户于 2026-09-30 批准冻结 |
| 备份生命周期 | active | partial | product | [backup-lifecycle.md](backup-lifecycle.md) | PLAN-0425；用户于 2026-09-30 批准冻结 |

术语区分：仓库逻辑归档由生命周期契约定义；目录快照/tar 备份由备份生命周期契约定义；pull 冲突备份由仓库生命周期定义触发语义。

## 3. 统一术语

各领域 SPEC 共用以下术语；领域文件不再重复定义，只引用此处。

| 术语 | 含义 |
|---|---|
| 操作者 | 注册 roots、设置策略和发起操作的决策主体。本产品为本地单操作者部署，操作者即本地人类操作者，与「人类用户」同义 |
| root（仓库根目录） | 操作者显式注册给 XS 的目录；XS 只扫描已注册 root，不隐式发现其他目录。clone 目标 root 与扫描 root 是同一概念 |
| 环境（测试/生产） | 相互隔离的存储根，各自拥有仓库目录、元数据库、索引和备份；跨环境写入必须拒绝。每个 root 注册时归属且仅归属一个环境 |
| XS 标签 | XS 自己的仓库分类标记，不是 Git tag、branch 或 commit |
| slug | 标签的稳定唯一标识（创建时指定，全库唯一），用于标签关联与排序 |
| kind | 仓库类型（如 fork/own、third-party）；在无 repo/tag 策略时提供默认策略 |
| effective policy（生效策略） | 按 repo > tag > kind 顺序解析出的单一有效策略；查询结果必须显示其来源 |
| 策略许可 | effective policy 决定某操作是否允许及采用何种冲突动作 |
| 人工确认 | 覆盖/丢失数据的单次操作需本地人类操作者逐次确认；与策略许可是两个独立的门 |
| 独立仓库备份 | 操作者主动创建、可列举、可恢复的仓库副本 |
| pull 冲突备份 | pull 冲突时自动保护旧仓库的机制；不进入用户备份列表，不计入用户备份数量 |
| 逻辑归档（logical archive） | 仓库退出默认活动视图的可逆状态；不创建备份文件，不删除仓库文件 |
| pre-restore recovery copy | 恢复前保留的目标仓库当前状态副本 |
| 索引状态 | 最近扫描/索引的结果、失败原因和结果是否可能陈旧 |

任务结果状态统一为四个：`success`（成功）、`skipped`（跳过，含因策略/确认被拒的 denied 语义）、`pending_confirmation`（待确认）、`failed`（失败）。各 SPEC 引用此枚举，不得另造同义词。

验收场景编号规则：product-scope 用 `P*`，repository-discovery 用 `D*`，tags-and-policies 用 `T*`，repository-lifecycle 用 `L*`，backup-lifecycle 用 `B*`。后续实现 PLAN 与 verify 记录按编号引用场景。

## 4. 读取规则

- Agent 处理 XS 产品范围任务时，先读取本文件，再读取相关 SPEC。
- `proposed` 表示尚未接受的目标草案；`active` 表示契约已接受，不证明相应能力已经完整实现。
- 当前实现事实以 XS 源码、CLI/API 契约和项目 DEV 文档为准；SPEC 与实现有差异时按活跃 PLAN 的验收门处理，不静默把目标描述成现状。
- 根级 SPEC 不进入 `docs/` 文档站；只有经用户确认的契约结论可被公开开发说明概述，不得维护冲突的第二份契约正文。
- 各领域 SPEC 章节要素完整，按推荐顺序排列：目的 → 参与者与入口 → 能力状态 → 契约 → 安全边界 → 验收场景 → 非目标；章节标题措辞可按领域微调（如「目的与概念」「角色与结果」）。
- 能力状态表统一使用三值：`已实现` / `部分实现` / `未实现`；细别与例外以括注补充，不新造状态词。
