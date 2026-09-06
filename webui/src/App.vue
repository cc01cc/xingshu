<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { Copy, Ellipsis, FolderOpen, Menu, X } from "@lucide/vue";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
} from "@/components/ui/sheet";
import { Card } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Skeleton } from "@/components/ui/skeleton";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip";
import { useTaskStore, type TaskType, type ConflictDetail } from "./task-store";

type Repo = {
  id: number;
  rootId: number;
  org: string;
  name: string;
  relPath: string;
  remoteUrl: string | null;
  defaultBranch: string | null;
  headCommit: string | null;
  repoKind: string;
  cloneStatus: string;
  sizeBytes: number;
  lastPullStatus: string | null;
  lockViolation: boolean;
  modifyLock: boolean;
  tags: string[];
};
type Tag = { id: number; slug: string; label: string; color: string | null };
type Root = { id: number; name: string; path: string; diskLabel: string | null; mountPoint: string | null; priority: number };
type Stats = { bytes: number; repositories: number; byKind: Record<string, number> };

const repos = ref<Repo[]>([]);
const tags = ref<Tag[]>([]);
const roots = ref<Root[]>([]);
const stats = ref<Stats>({ bytes: 0, repositories: 0, byKind: {} });
const policyRepoId = ref<number | null>(null);
const policyForm = ref({ pullStrategy: "fetch-only", conflict: "stop", unattended: "abort" });
const fetchLogs = ref<Array<{id:number, strategy:string, result:string, startedAt:string}>>([]);
const moveTarget = ref<number | null>(null);
const pendingKind = ref<string | null>(null);
const kindConfirmOpen = ref(false);
const query = ref("");
const tagFilter = ref("");
const view = ref<"repos" | "tags" | "disks" | "settings" | "policy">("repos");
const selected = ref<Repo | null>(null);
const newTag = ref("");
const newRootPath = ref("");
const mobileNavOpen = ref(false);
const copyFeedback = ref("");
const loading = ref(true);
const error = ref("");
const taskStore = useTaskStore();
const {
  task,
  taskConflicts,
  connectionLabel,
  error: taskError,
  progressPercent,
  isBusy,
} = taskStore;
const scanning = computed(() => isBusy.value && task.value?.type === "scan");
const pulling = computed(() => isBusy.value && task.value?.type === "pull");
const resolvingAction = ref<"backup" | "overwrite" | "abort" | null>(null);
const overwriteConfirm = ref<null | { scope: "drawer" | "task"; repoId: number }>(null);
const taskProgressLabel = computed(() => {
  const current = task.value?.progressCurrent ?? 0;
  const total = task.value?.progressTotal;
  return total === null || total === undefined ? `已处理 ${current} 项` : `${current} / ${total}`;
});
const taskHasConflict = computed(() => taskConflicts.value.length > 0);

const parsedTaskResult = computed<Record<string, unknown> | null>(() => {
  const raw = task.value?.resultJson;
  if (!raw) return null;
  try {
    return JSON.parse(raw) as Record<string, unknown>;
  } catch {
    return null;
  }
});
const prettyTaskJson = computed(() => {
  const parsed = parsedTaskResult.value;
  if (!parsed) return task.value?.resultJson ?? "";
  try {
    return JSON.stringify(parsed, null, 2);
  } catch {
    return task.value?.resultJson ?? "";
  }
});
const taskCounts = computed(() => {
  const parsed = parsedTaskResult.value;
  if (!parsed || typeof parsed.counts !== "object" || parsed.counts === null || Array.isArray(parsed.counts)) return null;
  return parsed.counts as Record<string, number>;
});
const taskRepos = computed(() => {
  const parsed = parsedTaskResult.value;
  if (!parsed || !Array.isArray((parsed as Record<string, unknown>).repos)) return null;
  return (parsed as Record<string, unknown>).repos as Array<Record<string, unknown>>;
});
const taskScanStats = computed(() => {
  const parsed = parsedTaskResult.value;
  if (!parsed) return null;
  if ("repos_found" in parsed || "roots_scanned" in parsed || "reposFound" in parsed) return parsed as Record<string, unknown>;
  return null;
});

function labelResult(value: unknown): string {
  const key = String(value ?? "");
  const map: Record<string, string> = { ok: "成功", aborted: "已中止", skipped: "已跳过", failed: "失败", conflict: "冲突需决策", waiting_decision: "等待决策", resolving: "处理中", completed: "已完成", interrupted: "已中断" };
  return map[key] ?? key;
}
function labelStatus(value: unknown): string {
  const key = String(value ?? "");
  const map: Record<string, string> = { pending: "等待中", running: "运行中", completed: "已完成", failed: "失败", aborted: "已中止", waiting_decision: "等待决策", resolving: "处理中", interrupted: "已中断", ok: "成功", ahead: "本地领先", skipped: "已跳过", conflict: "冲突" };
  return map[key] ?? key;
}
function aheadBadgeTitle(repo: Repo): string {
  return repo.repoKind === "own" || repo.repoKind === "fork"
    ? "本地有未推送提交；如需推送请打开目录手动操作"
    : "本地领先上游：可能存在未推送提交，或上游已 force-push 回退（疑似）。可覆盖跟随上游或打开目录核实";
}
function conflictHeadline(detail: ConflictDetail): string {
  if (detail.kind === "dirty") return "工作区有未提交变更";
  if (detail.kind === "diverged") return "本地与上游历史分叉";
  return "上游历史被重写（non-fast-forward）";
}
function conflictOutcomeLines(): Array<{ action: string; text: string }> {
  return [
    { action: "备份后拉取", text: "整仓快照至 .bak.<时间戳> 后重新克隆，本地变更随备份保留" },
    { action: "覆盖本地", text: "reset --hard + clean -fd 后拉取，丢弃全部变更（含未跟踪文件），不可恢复" },
    { action: "保持现状", text: "不做任何操作，仓库维持当前状态" },
  ];
}
async function openDirectory(path: string): Promise<void> {
  try {
    await request("/api/v1/open", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ path }) });
  } catch (reason) {
    error.value = friendlyError(reason instanceof Error ? reason.message : "打开目录失败");
  }
}
function repoAbsolutePath(repo: Repo): string | null {
  const root = roots.value.find((item) => item.id === repo.rootId);
  if (!root) return null;
  return `${root.path.replace(/[\\/]+$/, "")}/${repo.relPath}`;
}
function requestOverwriteConfirm(scope: "drawer" | "task", repoId: number): void {
  overwriteConfirm.value = { scope, repoId };
}
function cancelOverwrite(): void {
  overwriteConfirm.value = null;
}
function formatDuration(value: unknown): string {
  if (value === null || value === undefined || value === "") return "—";
  const n = Number(value);
  if (!Number.isFinite(n)) return String(value);
  if (n < 1000) return `${n}ms`;
  if (n < 60000) return `${(n / 1000).toFixed(1)}s`;
  return `${(n / 60000).toFixed(1)}min`;
}
function formatFetchTime(value: string): string {
  return value.slice(0, 19).replace("T", " ");
}

function shortBackupPath(value: unknown): string {
  if (!value) return "—";
  const str = String(value);
  const parts = str.split(/[\\/]/);
  return parts[parts.length - 1] || str;
}
function friendlyError(message: string): string {
  if (message === "Failed to fetch" || message.includes("Failed to fetch")) {
    return "无法连接本地服务 (Failed to fetch)，请确认 xingshu-server 运行于 12681";
  }
  if (message.startsWith("HTTP 409")) return "任务冲突：已有同类任务正在运行";
  if (message.startsWith("HTTP 404")) return "未找到资源 (404)，请刷新后重试";
  return message;
}
function badgeClassForResult(value: unknown): string {
  const key = String(value ?? "");
  if (key === "failed" || key === "interrupted" || key === "error") return "error";
  if (key === "aborted" || key === "conflict" || key === "waiting_decision" || key === "resolving") return "warning";
  if (key === "ok" || key === "completed" || key === "success") return "success";
  return "";
}
function copyText(text: string): void {
  if (!text || text === "—") return;
  navigator.clipboard.writeText(text).then(() => {
    copyFeedback.value = "已复制";
    setTimeout(() => (copyFeedback.value = ""), 1500);
  }).catch(() => {
    copyFeedback.value = "复制失败";
    setTimeout(() => (copyFeedback.value = ""), 1500);
  });
}
function toggleMobileNav(): void { mobileNavOpen.value = !mobileNavOpen.value; }
function closeMobileNav(): void { mobileNavOpen.value = false; }
const taskTotal = computed(() => {
  if (!taskCounts.value) return null;
  return Object.values(taskCounts.value).reduce((sum, n) => sum + (Number(n) || 0), 0);
});
const taskSummaryText = computed(() => {
  if (!taskCounts.value || taskTotal.value === null) return "";
  const entries = Object.entries(taskCounts.value).map(([k, v]) => `${labelResult(k)} ${v}`);
  return `共 ${taskTotal.value} 个仓库：${entries.join("、")}`;
});

const filteredRepos = computed(() => repos.value.filter((repo) => {
  const needle = query.value.trim().toLowerCase();
  const matchesQuery = !needle || `${repo.org}/${repo.name} ${repo.relPath}`.toLowerCase().includes(needle);
  const matchesTag = !tagFilter.value || repo.tags.includes(tagFilter.value);
  return matchesQuery && matchesTag;
}));
const conflicts = computed(() => repos.value.filter((repo) => repo.lockViolation || repo.lastPullStatus === "conflict"));

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes;
  let unit = "B";
  for (const candidate of units) { value /= 1024; unit = candidate; if (value < 1024) break; }
  return `${value.toFixed(value >= 10 ? 0 : 1)} ${unit}`;
}

async function load(): Promise<void> {
  loading.value = true;
  try {
    const [repoResponse, tagResponse, statsResponse, rootResponse] = await Promise.all([
      fetch("/api/v1/repos"), fetch("/api/v1/tags"), fetch("/api/v1/stats"), fetch("/api/v1/roots"),
    ]);
    if (!repoResponse.ok || !tagResponse.ok || !statsResponse.ok || !rootResponse.ok) throw new Error("API 请求失败");
    repos.value = await repoResponse.json() as Repo[];
    tags.value = await tagResponse.json() as Tag[];
    stats.value = await statsResponse.json() as Stats;
    roots.value = await rootResponse.json() as Root[];
    error.value = "";
  } catch (reason) {
    const message = reason instanceof Error ? reason.message : "无法加载索引";
    error.value = friendlyError(message);
  } finally { loading.value = false; }
}

async function request(url: string, options?: RequestInit): Promise<Response> {
  const response = await fetch(url, options);
  if (!response.ok && response.status !== 409) throw new Error(`HTTP ${response.status}`);
  return response;
}

async function pull(repo: Repo): Promise<void> {
  try {
    const response = await request(`/api/v1/repos/${repo.id}/pull`, { method: "POST" });
    if (response.status === 409) selected.value = repo;
    await load();
  } catch (reason) { error.value = friendlyError(reason instanceof Error ? reason.message : "pull 失败"); }
}

async function resolveConflict(action: "backup" | "overwrite" | "abort"): Promise<void> {
  if (!selected.value || resolvingAction.value) return;
  resolvingAction.value = action;
  error.value = "";
  try {
    const response = await request(`/api/v1/repos/${selected.value.id}/pull/${action}`, { method: "POST" });
    // request throws only for non-2xx non-409; parse body for detail if needed
    try {
      const body: unknown = await response.clone().json();
      if (body && typeof body === "object" && "backupPath" in (body as Record<string, unknown>)) {
        const bp = (body as Record<string, unknown>).backupPath;
        if (bp) console.info("backup created", bp);
      }
    } catch { /* ignore json parse */ }
    selected.value = null;
    await load();
  } catch (reason) {
    const message = reason instanceof Error ? reason.message : "冲突处理失败";
    // improve problem-details visibility: fetch already maps detail
    error.value = friendlyError(message);
  } finally {
    resolvingAction.value = null;
  }
}

async function createTag(): Promise<void> {
  const slug = newTag.value.trim();
  if (!slug) return;
  await request("/api/v1/tags", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ slug, label: slug }) });
  newTag.value = "";
  await load();
}

async function attachTag(repo: Repo, slug: string): Promise<void> {
  if (!slug || repo.tags.includes(slug)) return;
  const repoId = repo.id;
  await request(`/api/v1/repos/${repo.id}/tags`, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ slug }) });
  await load();
  // Guard: drawer may have been closed (or switched repo) while awaiting;
  // never resurrect a dismissed selection with stale async results.
  if (selected.value?.id === repoId) selected.value = repos.value.find((item) => item.id === repoId) ?? selected.value;
}

function requestKindChange(value: unknown): void {
  const next = String(value ?? "");
  if (!selected.value || !next || next === selected.value.repoKind) return;
  pendingKind.value = next;
  kindConfirmOpen.value = true;
}

async function confirmKindChange(): Promise<void> {
  kindConfirmOpen.value = false;
  if (!pendingKind.value) return;
  const next = pendingKind.value;
  pendingKind.value = null;
  await updateKind(next);
}

function kindChangeHint(kind: string): string {
  return kind === "third-party" || kind === "third-party-frozen"
    ? "切换为只读约定类型：出现本地改动会被标为待决策"
    : "切换为可写类型：将解除只读约定标记，本地改动不再告警";
}

function filterByTag(slug: string): void {
  tagFilter.value = slug;
  view.value = "repos";
}

async function addRoot(): Promise<void> {
  const path = newRootPath.value.trim();
  if (!path) return;
  try {
    await request("/api/v1/roots", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ path }) });
    newRootPath.value = "";
    await load();
  } catch (reason) { error.value = friendlyError(reason instanceof Error ? reason.message : "添加根目录失败"); }
}

async function removeRoot(id: number): Promise<void> {
  try {
    await request(`/api/v1/roots/${id}`, { method: "DELETE" });
    await load();
  } catch (reason) { error.value = friendlyError(reason instanceof Error ? reason.message : "删除根目录失败"); }
}

async function updateKind(kind: string): Promise<void> {
  if (!selected.value) return;
  const repoId = selected.value.id;
  try {
    await request(`/api/v1/repos/${selected.value.id}`, { method: "PUT", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ kind }) });
    await load();
    if (selected.value?.id === repoId) selected.value = repos.value.find((item) => item.id === repoId) ?? selected.value;
  } catch (reason) { error.value = friendlyError(reason instanceof Error ? reason.message : "更新类型失败"); }
}

async function deleteTag(id: number): Promise<void> {
  try {
    await request(`/api/v1/tags/${id}`, { method: "DELETE" });
    await load();
  } catch (reason) { error.value = friendlyError(reason instanceof Error ? reason.message : "删除标签失败"); }
}

async function loadPolicy(repoId: number): Promise<void> {
  try {
    const r = await request(`/api/v1/repos/${repoId}/policy`);
    const j = await r.json() as Record<string, unknown>;
    policyRepoId.value = repoId;
    policyForm.value = {
      pullStrategy: (j.pullStrategy as string) ?? (j.pull_strategy as string) ?? "fetch-only",
      conflict: (j.pullConflictPolicy as string) ?? (j.pull_conflict_policy as string) ?? "stop",
      unattended: (j.unattendedConflictPolicy as string) ?? (j.unattended_conflict_policy as string) ?? "abort",
    };
  } catch (reason) { error.value = friendlyError(reason instanceof Error ? reason.message : "加载策略失败"); }
}

async function savePolicy(): Promise<void> {
  if (policyRepoId.value === null) return;
  try {
    await request(`/api/v1/repos/${policyRepoId.value}/policy`, { method: "PUT", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ pullStrategy: policyForm.value.pullStrategy, conflict: policyForm.value.conflict, unattended: policyForm.value.unattended }) });
    error.value = "策略已保存";
    setTimeout(() => (error.value = ""), 1500);
  } catch (reason) { error.value = friendlyError(reason instanceof Error ? reason.message : "保存策略失败"); }
}

async function loadFetchLogs(repoId: number): Promise<void> {
  try {
    const r = await request(`/api/v1/repos/${repoId}/fetch_log`);
    const j = await r.json() as Array<Record<string, unknown>>;
    fetchLogs.value = j.map((x) => ({ id: Number(x.id), strategy: String(x.strategy ?? ""), result: String(x.result ?? ""), startedAt: String(x.startedAt ?? x.started_at ?? "") }));
  } catch { fetchLogs.value = []; }
}

async function moveRepo(): Promise<void> {
  if (!selected.value || moveTarget.value === null) return;
  const repoId = selected.value.id;
  try {
    await request(`/api/v1/repos/${selected.value.id}/move`, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ targetRootId: moveTarget.value }) });
    await load();
    if (selected.value?.id === repoId) selected.value = repos.value.find((item) => item.id === repoId) ?? selected.value;
    error.value = "搬迁成功";
    setTimeout(() => (error.value = ""), 1500);
  } catch (reason) { error.value = friendlyError(reason instanceof Error ? reason.message : "搬迁失败"); }
}

async function triggerScan(): Promise<void> {
  await triggerTask("scan");
}

async function triggerPull(): Promise<void> {
  await triggerTask("pull");
}

async function triggerTask(type: TaskType): Promise<void> {
  error.value = "";
  try {
    await taskStore.start(type);
  } catch (reason) {
    error.value = friendlyError(reason instanceof Error ? reason.message : `${type} 任务启动失败`);
  }
}

async function decideTaskConflict(
  repoId: number,
  action: "backup" | "overwrite" | "abort",
): Promise<void> {
  error.value = "";
  try {
    await taskStore.decide(repoId, action);
  } catch (reason) {
    error.value = friendlyError(reason instanceof Error ? reason.message : "冲突决策失败");
  }
}

function setView(nextView: "repos" | "tags" | "disks" | "settings" | "policy"): void {
  view.value = nextView;
  closeMobileNav();
}

onMounted(() => {
  void load();
  const onKey = (e: KeyboardEvent): void => {
    if (e.key === "Escape") {
      if (selected.value) selected.value = null;
      if (mobileNavOpen.value) mobileNavOpen.value = false;
    }
  };
  window.addEventListener("keydown", onKey);
});

watch(() => task.value?.status, (status, previous) => {
  if (status && previous && status !== previous && ["completed", "failed", "interrupted"].includes(status)) {
    void load();
  }
});
</script>

<template>
  <TooltipProvider>
  <div class="shell style-nova">
    <aside class="sidebar" :class="{open: mobileNavOpen}">
      <div class="brand"><span class="brand-mark">✦</span><span><strong>星枢</strong><small>XINGSHU</small></span></div>
      <nav>
        <a :class="{ active: view === 'repos' }" href="#" @click.prevent="setView('repos')">仓库 <span>{{ repos.length }}</span></a>
        <a :class="{ active: view === 'tags' }" href="#" @click.prevent="setView('tags')">标签 <span :class="{ 'count-zero': tags.length===0 }">{{ tags.length }}</span></a>
        <a :class="{ active: view === 'disks' }" href="#" @click.prevent="setView('disks')">磁盘看板</a>
        <a :class="{ active: view === 'settings' }" href="#" @click.prevent="setView('settings')">设置</a>
        <a :class="{ active: view === 'policy' }" href="#" @click.prevent="setView('policy')">策略</a>
        <a v-if="conflicts.length" class="attention" href="#" @click.prevent="setView('repos')">待决策 <span>{{ conflicts.length }}</span></a>
         <Button variant="outline" size="sm" class="mt-3 w-full" :disabled="isBusy" @click="void triggerScan()">{{ scanning ? '扫描中…' : '扫描索引' }}</Button>
         <Button variant="outline" size="sm" class="mt-2 w-full" :disabled="isBusy" @click="void triggerPull()">{{ pulling ? '批量 pull 中…' : '批量 pull' }}</Button>
      </nav>
      <div class="sidebar-note">本地仓库索引<br /><small>localhost only</small></div>
    </aside>
    <div v-if="mobileNavOpen" class="sidebar-backdrop" @click="closeMobileNav"></div>
    <main class="main">
      <header class="toolbar">
        <Button variant="ghost" size="icon" class="hamburger" aria-label="切换导航" @click="toggleMobileNav"><Menu class="size-4" /></Button>
        <div><p class="eyebrow">REPOSITORY INDEX</p><h1>{{ view === 'repos' ? '仓库目录' : view === 'tags' ? '主题标签' : view === 'disks' ? '磁盘看板' : view === 'policy' ? '策略' : '设置' }}</h1></div>
        <Input v-if="view === 'repos'" v-model="query" aria-label="搜索仓库" placeholder="搜索仓库、组织或路径…" />
        <span v-if="copyFeedback" class="copy-feedback" style="color:#d6aa68;font-size:11px;margin-left:8px;">{{ copyFeedback }}</span>
      </header>
        <section v-if="task" class="task-panel" aria-live="polite">
          <div class="task-heading"><div><p class="eyebrow">ASYNC TASK</p><strong>{{ task.type === 'scan' ? '索引扫描' : '批量 pull' }}</strong><Badge variant="outline" class="task-status" :class="task.status">{{ labelStatus(task.status) }}</Badge></div><Button v-if="!isBusy" variant="ghost" size="sm" @click="taskStore.dismiss">关闭</Button></div>
          <div class="task-meta"><span>{{ connectionLabel }}</span><span>{{ taskProgressLabel }}</span><span v-if="task.lastRepo" class="mono" :title="task.lastRepo">{{ task.lastRepo }}</span></div>
          <div class="task-progress" :class="{ indeterminate: isBusy && progressPercent === null }"><span :style="progressPercent === null ? (task?.status === 'completed' ? { width: '100%' } : isBusy ? undefined : { width: '100%', opacity: '0.5' }) : { width: `${progressPercent}%` }" /></div>
          <p v-if="task.lastResult" class="task-result">{{ labelStatus(task.lastResult) }}</p>
          <div v-if="taskConflicts.length" class="task-conflicts">
            <div class="task-conflicts-heading"><strong>待决策仓库</strong><span>{{ taskConflicts.length }} 项</span></div>
            <article v-for="conflict in taskConflicts" :key="conflict.repoId" class="task-conflict">
              <div><strong>{{ conflict.repoName }}</strong><small class="path">{{ conflict.repoPath }}</small><small v-if="conflict.backupPath" class="path" :title="String(conflict.backupPath)">备份: {{ shortBackupPath(conflict.backupPath) }}<Button v-if="conflict.backupPath" variant="ghost" size="icon-sm" class="copy-btn" @click="void openDirectory(String(conflict.backupPath))" title="打开备份所在目录"><FolderOpen class="size-4" /></Button></small></div>
              <template v-if="conflict.conflictDetail">
                <p><b>{{ conflictHeadline(conflict.conflictDetail) }}</b> — {{ conflict.conflictDetail.reason }}</p>
                <div v-if="conflict.conflictDetail.kind === 'dirty'" class="conflict-detail">
                  <p class="conflict-overview">已暂存 {{ conflict.conflictDetail.staged }} · 已修改 {{ conflict.conflictDetail.modified }} · 未跟踪 {{ conflict.conflictDetail.untracked }}（共 {{ conflict.conflictDetail.statusEntries.length }} 个文件）</p>
                  <ul class="conflict-files">
                    <li v-for="entry in conflict.conflictDetail.statusEntries.slice(0, 10)" :key="entry.path" class="mono" :title="entry.path"><span class="status-code">{{ entry.index.trim() || '-' }}{{ entry.worktree.trim() || '-' }}</span> {{ entry.path }}</li>
                    <li v-if="conflict.conflictDetail.statusEntries.length > 10" class="muted">… 共 {{ conflict.conflictDetail.statusEntries.length }} 个文件</li>
                  </ul>
                </div>
                <div v-else class="conflict-detail">
                  <p class="conflict-overview">分叉点 <span class="mono">{{ conflict.conflictDetail.mergeBase ?? '?' }}</span><span v-if="conflict.conflictDetail.mergeBaseDate"> ({{ conflict.conflictDetail.mergeBaseDate }})</span> · 本地独有 {{ conflict.conflictDetail.localOnly.length }} 个提交 · 上游新增 {{ conflict.conflictDetail.upstreamOnly.length }} 个提交；fast-forward 不可行</p>
                  <div class="conflict-columns">
                    <div><b>本地独有</b><ul class="conflict-files"><li v-for="commit in conflict.conflictDetail.localOnly.slice(0, 20)" :key="commit.short" :title="`${commit.author} ${commit.date}`"><span class="mono">{{ commit.short }}</span> {{ commit.summary }}</li></ul></div>
                    <div><b>上游新增</b><ul class="conflict-files"><li v-for="commit in conflict.conflictDetail.upstreamOnly.slice(0, 20)" :key="commit.short" :title="`${commit.author} ${commit.date}`"><span class="mono">{{ commit.short }}</span> {{ commit.summary }}</li></ul></div>
                  </div>
                  <p v-for="pair in conflict.conflictDetail.equivalent" :key="pair[0]" class="conflict-equivalent">ⓘ {{ pair[0] }} ≡ 上游 {{ pair[1] }}（内容相同，疑似回退后重做/rebase）</p>
                </div>
                <details class="conflict-outcomes"><summary>各动作后果</summary><ul><li v-for="line in conflictOutcomeLines()" :key="line.action"><b>{{ line.action }}</b>：{{ line.text }}</li></ul></details>
              </template>
              <p v-else>{{ conflict.conflictReason ?? 'Git 操作需要人工决策' }}</p>
              <p class="mono" style="color:#6d7671;font-size:10px">耗时 {{ formatDuration(conflict.durationMs) }} · 状态 {{ labelStatus(conflict.status) }}<span v-if="conflict.requestedAction" style="margin-left:6px">· 已请求: {{ labelResult(conflict.requestedAction) }}</span></p>
              <div class="task-conflict-actions">
                <Button size="sm" :disabled="conflict.status !== 'waiting_decision'" @click="void decideTaskConflict(conflict.repoId, 'backup')">备份后拉取</Button>
                <Button v-if="overwriteConfirm?.scope === 'task' && overwriteConfirm.repoId === conflict.repoId" size="sm" variant="destructive" :disabled="conflict.status !== 'waiting_decision'" @click="void decideTaskConflict(conflict.repoId, 'overwrite'); cancelOverwrite()">确认覆盖（丢弃本地变更，不可恢复）</Button>
                <Button v-else size="sm" variant="outline" :disabled="conflict.status !== 'waiting_decision'" @click="requestOverwriteConfirm('task', conflict.repoId)">覆盖本地</Button>
                <Button size="sm" variant="ghost" :disabled="conflict.status !== 'waiting_decision'" @click="void decideTaskConflict(conflict.repoId, 'abort')">保持现状</Button>
              </div>
            </article>
          </div>
          <button v-if="taskHasConflict" class="task-link" @click="setView('repos')">查看冲突仓</button>
          <details v-if="task.resultJson" class="task-details" open><summary>任务结果</summary>
            <div v-if="parsedTaskResult" class="task-result-rendered">
              <p v-if="taskSummaryText" class="task-summary">{{ taskSummaryText }}</p>
              <div v-if="taskCounts" class="task-counts">
                <span v-for="(count, key) in taskCounts" :key="String(key)" class="count-badge" :class="'count-' + String(key)" :title="String(key)">{{ labelResult(key) }} {{ count }}</span>
              </div>
              <div v-if="taskRepos && taskRepos.length" class="task-repos-wrap">
                <Table class="task-repos-table">
                  <TableHeader><TableRow><TableHead>仓库</TableHead><TableHead>状态</TableHead><TableHead>结果</TableHead><TableHead>耗时</TableHead><TableHead>备份</TableHead><TableHead>错误</TableHead></TableRow></TableHeader>
                  <TableBody>
                    <TableRow v-for="repo in taskRepos" :key="String((repo as Record<string, unknown>).repoId ?? (repo as Record<string, unknown>).repo ?? Math.random())">
                      <TableCell class="mono" :title="String((repo as Record<string, unknown>).repo ?? (repo as Record<string, unknown>).repoId ?? '')">{{ String((repo as Record<string, unknown>).repo ?? (repo as Record<string, unknown>).repoId ?? '—') }}</TableCell>
                      <TableCell>{{ labelStatus((repo as Record<string, unknown>).status) }}</TableCell>
                      <TableCell><Badge variant="outline" :class="['status', badgeClassForResult((repo as Record<string, unknown>).result)]">{{ labelResult((repo as Record<string, unknown>).result) }}</Badge></TableCell>
                      <TableCell class="mono">{{ formatDuration((repo as Record<string, unknown>).durationMs) }}</TableCell>
                      <TableCell class="mono" :title="String((repo as Record<string, unknown>).backupPath ?? '')">{{ shortBackupPath((repo as Record<string, unknown>).backupPath) }}</TableCell>
                      <TableCell class="mono error-cell" :title="String((repo as Record<string, unknown>).error ?? '')">{{ (repo as Record<string, unknown>).error ? String((repo as Record<string, unknown>).error).slice(0, 80) : '—' }}</TableCell>
                    </TableRow>
                  </TableBody>
                </Table>
              </div>
              <div v-else-if="taskScanStats" class="task-scan-stats">
                <span>已扫描 {{ String((taskScanStats as Record<string, unknown>).roots_scanned ?? (taskScanStats as Record<string, unknown>).rootsScanned ?? '—') }} 根</span>
                <span>发现 {{ String((taskScanStats as Record<string, unknown>).repos_found ?? (taskScanStats as Record<string, unknown>).reposFound ?? '—') }} 仓</span>
                <span v-if="(taskScanStats as Record<string, unknown>).broken_repos || (taskScanStats as Record<string, unknown>).brokenRepos">异常 {{ String((taskScanStats as Record<string, unknown>).broken_repos ?? (taskScanStats as Record<string, unknown>).brokenRepos) }}</span>
                <span v-if="(taskScanStats as Record<string, unknown>).skipped_directories || (taskScanStats as Record<string, unknown>).skippedDirectories">跳过 {{ String((taskScanStats as Record<string, unknown>).skipped_directories ?? (taskScanStats as Record<string, unknown>).skippedDirectories) }}</span>
                <span v-if="(taskScanStats as Record<string, unknown>).nested_repos || (taskScanStats as Record<string, unknown>).nestedRepos">嵌套 {{ String((taskScanStats as Record<string, unknown>).nested_repos ?? (taskScanStats as Record<string, unknown>).nestedRepos) }}</span>
              </div>
              <details class="task-raw"><summary>原始 JSON（调试）</summary><pre class="task-json">{{ prettyTaskJson }}</pre></details>
            </div>
            <code v-else class="task-raw-fallback">{{ task.resultJson }}</code>
          </details>
          <p v-if="taskError || task.error" class="task-error">{{ taskError || task.error }}</p>
       </section>
       <p v-if="error" class="notice error">{{ error }}</p>
      <div v-else-if="loading">
          <Skeleton style="height:16px;width:40%;margin-top:42px;" />
          <Skeleton style="height:48px;margin-top:12px;" />
          <Skeleton style="height:48px;margin-top:8px;" />
          <Skeleton style="height:48px;margin-top:8px;" />
        </div>
      <template v-else-if="view === 'repos'">
        <section class="summary"><span><b>{{ filteredRepos.length }}</b> 个结果</span><Select :model-value="tagFilter || '__all'" @update:model-value="tagFilter = $event === '__all' ? '' : String($event ?? '')"><SelectTrigger class="w-44" aria-label="按标签过滤"><SelectValue placeholder="全部标签" /></SelectTrigger><SelectContent><SelectItem value="__all">全部标签</SelectItem><SelectItem v-for="tag in tags" :key="tag.id" :value="tag.slug">{{ tag.label }}</SelectItem></SelectContent></Select><span class="legend"><i class="dot safe" />索引正常 <i class="dot warn" />需要处理</span></section>
        <div v-if="filteredRepos.length === 0" class="empty">
          <template v-if="!repos.length && roots.length">暂无仓库索引 · 请到 <a href="#" @click.prevent="setView('settings')">设置</a> 点击「扫描索引」</template>
          <template v-else-if="!repos.length && !roots.length">暂无根目录 · 请到 <a href="#" @click.prevent="setView('settings')">设置</a> 添加根目录后扫描</template>
          <template v-else>没有匹配的仓库</template>
        </div>
        <div v-else class="table-wrap"><Table aria-label="仓库索引列表"><TableHeader><TableRow><TableHead>仓库</TableHead><TableHead>类型</TableHead><TableHead>标签</TableHead><TableHead>大小</TableHead><TableHead>状态</TableHead><TableHead /></TableRow></TableHeader><TableBody><TableRow v-for="repo in filteredRepos" :key="repo.id" :class="{selected: selected?.id===repo.id}" @click="selected = repo" tabindex="0" @keydown.enter="selected = repo" @keydown.space.prevent="selected = repo" :aria-selected="selected?.id===repo.id">
          <TableCell><strong :title="`${repo.org}/${repo.name}`">{{ repo.org }}/{{ repo.name }}</strong><small :title="repo.relPath">{{ repo.relPath }}<Button variant="ghost" size="icon-sm" class="copy-btn" @click.stop="copyText(repo.relPath)" title="复制路径"><Copy class="size-4" /></Button></small></TableCell><TableCell><Badge variant="outline" class="kind" :class="repo.repoKind">{{ repo.repoKind }}</Badge></TableCell><TableCell><Badge v-for="slug in repo.tags" :key="slug" variant="secondary">{{ slug }}</Badge><span v-if="!repo.tags.length" class="muted">未分类</span></TableCell><TableCell class="mono">{{ formatBytes(repo.sizeBytes) }}</TableCell><TableCell><Badge v-if="repo.cloneStatus==='broken'" variant="destructive">异常</Badge><Badge v-else-if="repo.lockViolation" variant="outline" class="status warning">本地改动</Badge><Badge v-else-if="repo.lastPullStatus==='ahead'" variant="outline" class="status warning" :title="aheadBadgeTitle(repo)">本地领先</Badge><Badge v-else-if="repo.lastPullStatus==='ok'" variant="outline" class="status success">正常</Badge><Badge v-else variant="outline">{{ repo.lastPullStatus ?? "未更新" }}</Badge></TableCell><TableCell><Button variant="ghost" size="icon" class="more" @click.stop="selected = repo"><Ellipsis class="size-4" /></Button></TableCell>
        </TableRow></TableBody></Table></div>
      </template>
      <section v-else-if="view === 'tags'" class="tag-grid"><form class="tag-create" @submit.prevent="void createTag()"><Input v-model="newTag" aria-label="新标签" placeholder="新建主题标签…" /><Button type="submit">添加</Button></form><Card v-for="tag in tags" :key="tag.id" data-size="sm" class="tag-card"><Button variant="ghost" size="icon-sm" class="tag-delete" @click.stop="deleteTag(tag.id)" aria-label="删除标签"><X class="size-4" /></Button><button class="tag-card-body" @click="filterByTag(tag.slug)"><b>{{ tag.label }}</b><small>{{ repos.filter((repo) => repo.tags.includes(tag.slug)).length }} 个仓库</small></button></Card></section>
      <template v-else-if="view === 'disks'">
        <section class="dashboard"><Card class="stat-card"><small>索引仓库</small><strong>{{ stats.repositories }}</strong></Card><Card class="stat-card"><small>已占用空间</small><strong>{{ formatBytes(stats.bytes) }}</strong></Card><Card class="stat-card"><small>待决策</small><strong class="accent">{{ conflicts.length }}</strong></Card></section>
        <div v-if="Object.keys(stats.byKind).length" class="kind-bar"><div v-for="(count, kind) in stats.byKind" :key="kind" class="kind-segment" :style="{ flex: count }"><Badge variant="outline" class="kind" :class="kind">{{ kind }}</Badge> {{ count }}</div></div>
        <div v-if="roots.length" style="margin-top:16px;display:grid;gap:8px;">
          <div v-for="root in roots" :key="root.id" class="root-item">
            <div><strong>{{ root.name }}</strong><small>{{ root.path }}</small></div>
            <span class="mono">{{ repos.filter(r=>r.rootId===root.id).length }} 仓</span>
          </div>
        </div>
      </template>
      <template v-else-if="view === 'settings'">
        <h2 class="settings-title">根目录管理</h2>
        <p class="settings-hint">添加本地 Git 仓库的父目录（可多个、跨盘），添加后需手动触发扫描才会生成索引。</p>
        <form class="root-create" @submit.prevent="addRoot"><Input v-model="newRootPath" aria-label="根目录路径" placeholder="输入根目录绝对路径…" /><Button type="submit">添加</Button></form>
        <div class="root-list"><div v-for="root in roots" :key="root.id" class="root-item"><div><strong>{{ root.name }}</strong><small>{{ root.path }}</small><Badge v-if="root.diskLabel" variant="secondary">{{ root.diskLabel }}</Badge></div><Button variant="ghost" size="sm" @click="removeRoot(root.id)">移除</Button></div><div v-if="!roots.length" class="empty">暂无根目录 · 示例：S:\zeogit-ref</div></div>
        <div class="settings-actions">
          <Button :disabled="isBusy || !roots.length" @click="void triggerScan()">{{ scanning ? '扫描中…' : `扫描索引（${roots.length} 个根目录）` }}</Button>
          <span class="muted">{{ roots.length ? (repos.length ? `已索引 ${repos.length} 个仓库` : '尚未扫描，点击扫描后仓库将出现在「仓库」页') : '先添加根目录' }}</span>
        </div>
        <p v-if="roots.length && !repos.length && !task" class="notice">提示：根目录已就绪，请点击上方「扫描索引」执行首次索引。Z:\TEST 这类空目录不会产生仓库。</p>
        <h2 class="settings-title">关于</h2>
        <dl class="about"><dt>版本</dt><dd>0.1.0</dd><dt>API 端口</dt><dd>12681</dd><dt>WebUI 端口</dt><dd>12680</dd></dl>
      </template>
      <template v-else-if="view === 'policy'">
        <h2 class="settings-title">策略管理</h2>
        <p class="settings-hint">按仓库配置拉取策略与冲突处理，空值回退为 kind 默认。</p>
        <div class="root-create">
          <Select :model-value="policyRepoId === null ? '' : String(policyRepoId)" @update:model-value="policyRepoId = $event === '' ? null : Number($event)">
            <SelectTrigger class="w-64" aria-label="选择仓库"><SelectValue placeholder="选择仓库…" /></SelectTrigger>
            <SelectContent><SelectItem v-for="repo in repos" :key="repo.id" :value="String(repo.id)">{{ repo.org }}/{{ repo.name }}</SelectItem></SelectContent>
          </Select>
          <Button variant="outline" size="sm" @click="policyRepoId!==null && loadPolicy(policyRepoId)">加载</Button>
        </div>
        <div v-if="policyRepoId!==null" class="policy-form" style="display:grid;gap:8px;max-width:480px;">
          <div class="policy-field"><Label>拉取策略</Label><Select v-model="policyForm.pullStrategy"><SelectTrigger class="w-44"><SelectValue /></SelectTrigger><SelectContent><SelectItem value="fetch-only">fetch-only</SelectItem><SelectItem value="mirror">mirror</SelectItem><SelectItem value="no-update">no-update</SelectItem><SelectItem value="archive">archive</SelectItem></SelectContent></Select></div>
          <div class="policy-field"><Label>冲突策略</Label><Select v-model="policyForm.conflict"><SelectTrigger class="w-44"><SelectValue /></SelectTrigger><SelectContent><SelectItem value="stop">stop</SelectItem><SelectItem value="backup">backup</SelectItem><SelectItem value="overwrite">overwrite</SelectItem><SelectItem value="abort">abort</SelectItem></SelectContent></Select></div>
          <div class="policy-field"><Label>无人值守</Label><Select v-model="policyForm.unattended"><SelectTrigger class="w-44"><SelectValue /></SelectTrigger><SelectContent><SelectItem value="stop">stop</SelectItem><SelectItem value="backup">backup</SelectItem><SelectItem value="overwrite">overwrite</SelectItem><SelectItem value="abort">abort</SelectItem></SelectContent></Select></div>
          <Button @click="savePolicy">保存策略</Button>
        </div>
      </template>
    </main>
    <Sheet :open="selected !== null" @update:open="(value: boolean) => { if (!value) selected = null; }">
    <SheetContent v-if="selected" class="drawer" :show-close-button="false"><Button variant="ghost" size="icon" class="close" aria-label="关闭详情" @click="selected = null"><X class="size-4" /></Button><SheetHeader class="drawer-head"><p class="eyebrow">REPOSITORY DETAIL</p><SheetTitle class="drawer-title">{{ selected.org }}/{{ selected.name }}</SheetTitle><SheetDescription class="sr-only">仓库详情与操作</SheetDescription></SheetHeader><small class="path" :title="selected.relPath">{{ selected.relPath }}<Button variant="ghost" size="icon-sm" class="copy-btn" @click="copyText(selected.relPath)" title="复制路径"><Copy class="size-4" /></Button></small><dl><dt>类型</dt><dd><Select :model-value="selected.repoKind" @update:model-value="requestKindChange($event)"><SelectTrigger class="w-44" aria-label="仓库类型"><SelectValue /></SelectTrigger><SelectContent><SelectItem value="third-party">third-party</SelectItem><SelectItem value="third-party-frozen">third-party-frozen</SelectItem><SelectItem value="fork">fork</SelectItem><SelectItem value="own">own</SelectItem></SelectContent></Select><AlertDialog v-model:open="kindConfirmOpen"><AlertDialogContent><AlertDialogHeader><AlertDialogTitle>切换仓库类型？</AlertDialogTitle><AlertDialogDescription>{{ pendingKind ? kindChangeHint(pendingKind) : '' }}该操作立即写入索引。</AlertDialogDescription></AlertDialogHeader><AlertDialogFooter><AlertDialogCancel>取消</AlertDialogCancel><AlertDialogAction @click="void confirmKindChange()">确认切换</AlertDialogAction></AlertDialogFooter></AlertDialogContent></AlertDialog><span v-if="selected.modifyLock" class="muted"> · 只读约定<Tooltip><TooltipTrigger as-child><span class="lock-hint" tabindex="0" aria-label="只读约定说明"> ?</span></TooltipTrigger><TooltipContent class="max-w-60">third-party 约定只读：出现本地改动会被标为待决策，pull 时需人工确认；不阻止你在文件管理器里直接修改。</TooltipContent></Tooltip></span></dd><dt>分支</dt><dd>{{ selected.defaultBranch ?? 'bare / unknown' }}</dd><dt>HEAD</dt><dd class="mono" :title="selected.headCommit ?? ''">{{ selected.headCommit?.slice(0, 10) ?? '—' }}<Button v-if="selected.headCommit" variant="ghost" size="icon-sm" class="copy-btn" @click="copyText(selected.headCommit!)" title="复制 HEAD"><Copy class="size-4" /></Button></dd><dt>远程</dt><dd class="path" :title="selected.remoteUrl ?? ''">{{ selected.remoteUrl ?? '—' }}<Button v-if="selected.remoteUrl" variant="ghost" size="icon-sm" class="copy-btn" @click="copyText(selected.remoteUrl!)" title="复制远程"><Copy class="size-4" /></Button></dd><dt>大小</dt><dd>{{ formatBytes(selected.sizeBytes) }}</dd><dt>标签</dt><dd><Badge v-for="slug in selected.tags" :key="slug" variant="secondary">{{ slug }}</Badge><span v-if="!selected.tags.length" class="muted">无</span></dd></dl><div style="margin-top:16px;display:flex;gap:6px;align-items:center;">
          <Select :model-value="moveTarget === null ? undefined : String(moveTarget)" @update:model-value="moveTarget = $event == null || $event === '' ? null : Number($event)"><SelectTrigger class="min-w-0 flex-1" aria-label="目标根"><SelectValue placeholder="移动至…" /></SelectTrigger><SelectContent><SelectItem v-for="root in roots" :key="root.id" :value="String(root.id)" :disabled="root.id===selected.rootId">{{ root.name }} ({{ root.path }})</SelectItem></SelectContent></Select>
          <Button size="sm" @click="moveRepo" :disabled="moveTarget===null">搬迁</Button>
        </div>
        <details class="fetch-log"><summary @click="selected && loadFetchLogs(selected.id)">拉取历史</summary><div v-if="fetchLogs.length" class="fetch-log-list"><div v-for="log in fetchLogs" :key="log.id" class="fetch-log-row"><span class="mono">{{ formatFetchTime(log.startedAt) }}</span><span class="mono">{{ log.strategy }}</span><Badge variant="outline">{{ labelResult(log.result) }}</Badge></div></div><div v-else class="muted">暂无历史</div></details>
        <div class="drawer-actions"><Button size="sm" @click="void pull(selected)">pull</Button><Button v-if="repoAbsolutePath(selected)" size="sm" variant="outline" @click="void openDirectory(repoAbsolutePath(selected)!)" title="在文件管理器中打开仓库目录">打开目录</Button><Select @update:model-value="void attachTag(selected, String($event ?? ''))"><SelectTrigger aria-label="添加标签"><SelectValue placeholder="添加标签…" /></SelectTrigger><SelectContent><SelectItem v-for="tag in tags" :key="tag.id" :value="tag.slug">{{ tag.label }}</SelectItem></SelectContent></Select></div><div v-if="selected.lockViolation" class="conflict"><b>检测到本地改动</b><p>当前仓库被标记为锁定类型。选择处理方式：</p><ul class="conflict-outcome-list"><li><b>备份后拉取</b>：整仓快照至 .bak.<时间戳> 后重新克隆，本地变更随备份保留</li><li><b>覆盖本地</b>：丢弃全部变更（含未跟踪文件），不可恢复</li><li><b>保持现状</b>：不做任何操作</li></ul><Button :disabled="!!resolvingAction" @click="void resolveConflict('backup')">{{ resolvingAction==='backup' ? '处理中…' : '备份后拉取' }}</Button><Button v-if="overwriteConfirm?.scope === 'drawer'" variant="destructive" :disabled="!!resolvingAction" @click="void resolveConflict('overwrite'); cancelOverwrite()">确认覆盖（丢弃本地变更，不可恢复）</Button><Button v-else variant="outline" :disabled="!!resolvingAction" @click="overwriteConfirm = { scope: 'drawer', repoId: selected.id }">{{ resolvingAction==='overwrite' ? '处理中…' : '覆盖本地' }}</Button><Button variant="ghost" :disabled="!!resolvingAction" @click="void resolveConflict('abort')">{{ resolvingAction==='abort' ? '处理中…' : '保持现状' }}</Button><p v-if="resolvingAction" class="muted" style="margin-top:8px">正在执行 {{ labelResult(resolvingAction) }}，大仓库可能耗时数秒…</p></div>
    </SheetContent>
    </Sheet>
  </div>
  </TooltipProvider>
</template>
