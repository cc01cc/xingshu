import type { ConflictDetail } from "../task-store";

export type RepoLike = {
  repoKind: string;
  statusCheckedAt: string | null;
};

export function labelResult(value: unknown): string {
  const key = String(value ?? "");
  const map: Record<string, string> = {
    ok: "成功",
    aborted: "已跳过",
    skipped: "已跳过",
    failed: "失败",
    conflict: "冲突需决策",
    waiting_decision: "等待决策",
    resolving: "处理中",
    completed: "已完成",
    interrupted: "已中断",
  };
  return map[key] ?? key;
}

export function labelStatus(value: unknown): string {
  const key = String(value ?? "");
  const map: Record<string, string> = {
    pending: "等待中",
    running: "运行中",
    completed: "已完成",
    failed: "失败",
    aborted: "已跳过",
    waiting_decision: "等待决策",
    resolving: "处理中",
    interrupted: "已中断",
    ok: "成功",
    ahead: "本地领先",
    skipped: "已跳过",
    conflict: "冲突",
  };
  return map[key] ?? key;
}

export function aheadBadgeTitle(repo: RepoLike): string {
  return repo.repoKind === "own" || repo.repoKind === "fork"
    ? "本地有未推送提交；如需推送请打开目录手动操作"
    : "本地领先上游：可能存在未推送提交，或上游已 force-push 回退（疑似）。可覆盖跟随上游或打开目录核实";
}

export function statusTitle(repo: RepoLike): string {
  const checked = repo.statusCheckedAt ? new Date(repo.statusCheckedAt).toLocaleString() : "未知";
  return `扫描时间 ${checked}；领先/落后基于本地已 fetch 的引用，未校验远端最新状态`;
}

export function conflictHeadline(detail: ConflictDetail): string {
  if (detail.kind === "dirty") return "工作区有未提交变更";
  if (detail.kind === "diverged") return "本地与上游历史分叉";
  return "上游历史被重写（non-fast-forward）";
}

export function conflictOutcomeLines(): Array<{ action: string; text: string }> {
  return [
    { action: "备份后拉取", text: "整仓快照至 .bak.<时间戳> 后重新克隆，本地变更随备份保留" },
    { action: "覆盖本地", text: "reset --hard + clean -fd 后拉取，丢弃全部变更（含未跟踪文件），不可恢复" },
    { action: "保持现状", text: "不做任何操作，仓库维持当前状态" },
  ];
}

export function kindChangeHint(kind: string): string {
  return kind === "third-party" || kind === "third-party-frozen"
    ? "切换为只读约定类型：出现本地改动会被标为待决策"
    : "切换为可写类型：将解除只读约定标记，本地改动不再告警";
}

export function formatDuration(value: unknown): string {
  if (value === null || value === undefined || value === "") return "—";
  const n = Number(value);
  if (!Number.isFinite(n)) return String(value);
  if (n < 1000) return `${n}ms`;
  if (n < 60000) return `${(n / 1000).toFixed(1)}s`;
  return `${(n / 60000).toFixed(1)}min`;
}

export function formatFetchTime(value: string): string {
  return value.slice(0, 19).replace("T", " ");
}

export function shortBackupPath(value: unknown): string {
  if (!value) return "—";
  const str = String(value);
  const parts = str.split(/[\\/]/);
  return parts[parts.length - 1] || str;
}

export function badgeClassForResult(value: unknown): string {
  const key = String(value ?? "");
  if (key === "failed" || key === "interrupted" || key === "error") return "error";
  if (key === "aborted" || key === "conflict" || key === "waiting_decision" || key === "resolving") return "warning";
  if (key === "ok" || key === "completed" || key === "success") return "success";
  return "";
}

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes;
  let unit = "B";
  for (const candidate of units) {
    value /= 1024;
    unit = candidate;
    if (value < 1024) break;
  }
  return `${value.toFixed(value >= 10 ? 0 : 1)} ${unit}`;
}

export function formatScanTime(status: { statusCheckedAt: string | null } | null): string {
  const at = status?.statusCheckedAt;
  return at ? new Date(at).toLocaleString() : "未知";
}

export function formatBackupTime(entry: { createdAt: string }): string {
  const raw = entry.createdAt;
  return raw.length >= 14
    ? `${raw.slice(4, 6)}-${raw.slice(6, 8)} ${raw.slice(8, 10)}:${raw.slice(10, 12)}`
    : raw;
}
