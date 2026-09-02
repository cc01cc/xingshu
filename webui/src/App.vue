<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useTaskStore, type TaskType } from "./task-store";

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
  const map: Record<string, string> = { pending: "等待中", running: "运行中", completed: "已完成", failed: "失败", aborted: "已中止", waiting_decision: "等待决策", resolving: "处理中", interrupted: "已中断", ok: "成功", skipped: "已跳过", conflict: "冲突" };
  return map[key] ?? key;
}
function formatDuration(value: unknown): string {
  if (value === null || value === undefined || value === "") return "—";
  const n = Number(value);
  if (!Number.isFinite(n)) return String(value);
  if (n < 1000) return `${n}ms`;
  if (n < 60000) return `${(n / 1000).toFixed(1)}s`;
  return `${(n / 60000).toFixed(1)}min`;
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
  await request(`/api/v1/repos/${repo.id}/tags`, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ slug }) });
  await load();
  selected.value = repos.value.find((item) => item.id === repo.id) ?? null;
}

function selectTag(event: Event, repo: Repo): void {
  const target = event.target;
  if (target instanceof HTMLSelectElement) void attachTag(repo, target.value);
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
  try {
    await request(`/api/v1/repos/${selected.value.id}`, { method: "PUT", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ kind }) });
    await load();
    selected.value = repos.value.find((item) => item.id === selected.value?.id) ?? null;
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
  try {
    await request(`/api/v1/repos/${selected.value.id}/move`, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ targetRootId: moveTarget.value }) });
    await load();
    selected.value = repos.value.find((item) => item.id === selected.value?.id) ?? null;
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
  <div class="shell">
    <aside class="sidebar" :class="{open: mobileNavOpen}">
      <div class="brand"><span class="brand-mark">✦</span><span><strong>星枢</strong><small>XINGSHU</small></span></div>
      <nav>
        <a :class="{ active: view === 'repos' }" href="#" @click.prevent="setView('repos')">仓库 <span>{{ repos.length }}</span></a>
        <a :class="{ active: view === 'tags' }" href="#" @click.prevent="setView('tags')">标签 <span>{{ tags.length }}</span></a>
        <a :class="{ active: view === 'disks' }" href="#" @click.prevent="setView('disks')">磁盘看板</a>
        <a :class="{ active: view === 'settings' }" href="#" @click.prevent="setView('settings')">设置</a>
        <a :class="{ active: view === 'policy' }" href="#" @click.prevent="setView('policy')">策略</a>
        <a v-if="conflicts.length" class="attention" href="#" @click.prevent="setView('repos')">待决策 <span>{{ conflicts.length }}</span></a>
         <button class="scan-btn" :disabled="isBusy" @click="void triggerScan()">{{ scanning ? '扫描中…' : '扫描索引' }}</button>
         <button class="scan-btn" :disabled="isBusy" @click="void triggerPull()">{{ pulling ? '批量 pull 中…' : '批量 pull' }}</button>
      </nav>
      <div class="sidebar-note">本地仓库索引<br /><small>localhost only</small></div>
    </aside>
    <div v-if="mobileNavOpen" class="sidebar-backdrop" @click="closeMobileNav"></div>
    <main class="main">
      <header class="toolbar">
        <button class="hamburger" aria-label="切换导航" @click="toggleMobileNav">☰</button>
        <div><p class="eyebrow">REPOSITORY INDEX</p><h1>{{ view === 'repos' ? '仓库目录' : view === 'tags' ? '主题标签' : view === 'disks' ? '磁盘看板' : view === 'policy' ? '策略' : '设置' }}</h1></div>
        <input v-if="view === 'repos'" v-model="query" aria-label="搜索仓库" placeholder="搜索仓库、组织或路径…" />
        <span v-if="copyFeedback" class="copy-feedback" style="color:#d6aa68;font-size:11px;margin-left:8px;">{{ copyFeedback }}</span>
      </header>
        <section v-if="task" class="task-panel" aria-live="polite">
          <div class="task-heading"><div><p class="eyebrow">ASYNC TASK</p><strong>{{ task.type === 'scan' ? '索引扫描' : '批量 pull' }}</strong><span class="task-status" :class="task.status">{{ labelStatus(task.status) }}</span></div><button v-if="!isBusy" class="task-dismiss" @click="taskStore.dismiss">关闭</button></div>
          <div class="task-meta"><span>{{ connectionLabel }}</span><span>{{ taskProgressLabel }}</span><span v-if="task.lastRepo" class="mono" :title="task.lastRepo">{{ task.lastRepo }}</span></div>
          <div class="task-progress" :class="{ indeterminate: isBusy && progressPercent === null }"><span :style="progressPercent === null ? (task?.status === 'completed' ? { width: '100%' } : isBusy ? undefined : { width: '100%', opacity: '0.5' }) : { width: `${progressPercent}%` }" /></div>
          <p v-if="task.lastResult" class="task-result">{{ labelStatus(task.lastResult) }}</p>
          <div v-if="taskConflicts.length" class="task-conflicts">
            <div class="task-conflicts-heading"><strong>待决策仓库</strong><span>{{ taskConflicts.length }} 项</span></div>
            <article v-for="conflict in taskConflicts" :key="conflict.repoId" class="task-conflict">
              <div><strong>{{ conflict.repoName }}</strong><small class="path">{{ conflict.repoPath }}</small><small v-if="conflict.backupPath" class="path" :title="String(conflict.backupPath)">备份: {{ shortBackupPath(conflict.backupPath) }}</small></div>
              <p>{{ conflict.conflictReason ?? 'Git 操作需要人工决策' }}</p>
              <p class="mono" style="color:#6d7671;font-size:10px">耗时 {{ formatDuration(conflict.durationMs) }} · 状态 {{ labelStatus(conflict.status) }}<span v-if="conflict.requestedAction" style="margin-left:6px">· 已请求: {{ labelResult(conflict.requestedAction) }}</span></p>
              <div class="task-conflict-actions">
                <button :disabled="conflict.status !== 'waiting_decision'" @click="void decideTaskConflict(conflict.repoId, 'backup')">备份后拉取</button>
                <button :disabled="conflict.status !== 'waiting_decision'" @click="void decideTaskConflict(conflict.repoId, 'overwrite')">覆盖本地</button>
                <button :disabled="conflict.status !== 'waiting_decision'" @click="void decideTaskConflict(conflict.repoId, 'abort')">保持现状</button>
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
                <table class="task-repos-table">
                  <thead><tr><th>仓库</th><th>状态</th><th>结果</th><th>耗时</th><th>备份</th><th>错误</th></tr></thead>
                  <tbody>
                    <tr v-for="repo in taskRepos" :key="String((repo as Record<string, unknown>).repoId ?? (repo as Record<string, unknown>).repo ?? Math.random())">
                      <td class="mono" :title="String((repo as Record<string, unknown>).repo ?? (repo as Record<string, unknown>).repoId ?? '')">{{ String((repo as Record<string, unknown>).repo ?? (repo as Record<string, unknown>).repoId ?? '—') }}</td>
                      <td>{{ labelStatus((repo as Record<string, unknown>).status) }}</td>
                      <td><span class="status" :class="badgeClassForResult((repo as Record<string, unknown>).result)">{{ labelResult((repo as Record<string, unknown>).result) }}</span></td>
                      <td class="mono">{{ formatDuration((repo as Record<string, unknown>).durationMs) }}</td>
                      <td class="mono" :title="String((repo as Record<string, unknown>).backupPath ?? '')">{{ shortBackupPath((repo as Record<string, unknown>).backupPath) }}</td>
                      <td class="mono error-cell" :title="String((repo as Record<string, unknown>).error ?? '')">{{ (repo as Record<string, unknown>).error ? String((repo as Record<string, unknown>).error).slice(0, 80) : '—' }}</td>
                    </tr>
                  </tbody>
                </table>
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
          <div class="skeleton" style="height:16px;width:40%;margin-top:42px;"></div>
          <div class="skeleton" style="height:48px;margin-top:12px;"></div>
          <div class="skeleton" style="height:48px;margin-top:8px;"></div>
          <div class="skeleton" style="height:48px;margin-top:8px;"></div>
        </div>
      <template v-else-if="view === 'repos'">
        <section class="summary"><span><b>{{ filteredRepos.length }}</b> 个结果</span><select v-model="tagFilter" aria-label="按标签过滤"><option value="">全部标签</option><option v-for="tag in tags" :key="tag.id" :value="tag.slug">{{ tag.label }}</option></select><span class="legend"><i class="dot safe" />索引正常 <i class="dot warn" />需要处理</span></section>
        <div v-if="filteredRepos.length === 0" class="empty">
          <template v-if="!repos.length && roots.length">暂无仓库索引 · 请到 <a href="#" @click.prevent="setView('settings')">设置</a> 点击「扫描索引」</template>
          <template v-else-if="!repos.length && !roots.length">暂无根目录 · 请到 <a href="#" @click.prevent="setView('settings')">设置</a> 添加根目录后扫描</template>
          <template v-else>没有匹配的仓库</template>
        </div>
        <div v-else class="table-wrap"><table><thead><tr><th>仓库</th><th>类型</th><th>标签</th><th>大小</th><th>状态</th><th /></tr></thead><tbody><tr v-for="repo in filteredRepos" :key="repo.id" @click="selected = repo">
          <td><strong :title="`${repo.org}/${repo.name}`">{{ repo.org }}/{{ repo.name }}</strong><small :title="repo.relPath">{{ repo.relPath }}<button class="copy-btn" @click.stop="copyText(repo.relPath)" title="复制路径">⎘</button></small></td><td><span class="kind" :class="repo.repoKind">{{ repo.repoKind }}</span></td><td><span v-for="slug in repo.tags" :key="slug" class="tag">{{ slug }}</span><span v-if="!repo.tags.length" class="muted">未分类</span></td><td class="mono">{{ formatBytes(repo.sizeBytes) }}</td><td><span v-if="repo.cloneStatus==='broken'" class="status error">异常</span><span v-else-if="repo.lockViolation" class="status warning">本地改动</span><span v-else class="status">{{ repo.lastPullStatus ?? "未更新" }}</span></td><td><button class="more" @click.stop="selected = repo">···</button></td>
        </tr></tbody></table></div>
      </template>
      <section v-else-if="view === 'tags'" class="tag-grid"><form class="tag-create" @submit.prevent="void createTag()"><input v-model="newTag" aria-label="新标签" placeholder="新建主题标签…" /><button type="submit">添加</button></form><div v-for="tag in tags" :key="tag.id" class="tag-card"><button class="tag-delete" @click.stop="deleteTag(tag.id)" aria-label="删除标签">×</button><button class="tag-card-body" @click="filterByTag(tag.slug)"><b>{{ tag.label }}</b><small>{{ repos.filter((repo) => repo.tags.includes(tag.slug)).length }} 个仓库</small></button></div></section>
      <template v-else-if="view === 'disks'">
        <section class="dashboard"><div class="stat-card"><small>索引仓库</small><strong>{{ stats.repositories }}</strong></div><div class="stat-card"><small>已占用空间</small><strong>{{ formatBytes(stats.bytes) }}</strong></div><div class="stat-card"><small>待决策</small><strong class="accent">{{ conflicts.length }}</strong></div></section>
        <div v-if="Object.keys(stats.byKind).length" class="kind-bar"><div v-for="(count, kind) in stats.byKind" :key="kind" class="kind-segment" :style="{ flex: count }"><span class="kind" :class="kind">{{ kind }}</span> {{ count }}</div></div>
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
        <form class="root-create" @submit.prevent="addRoot"><input v-model="newRootPath" aria-label="根目录路径" placeholder="输入根目录绝对路径…" /><button type="submit">添加</button></form>
        <div class="root-list"><div v-for="root in roots" :key="root.id" class="root-item"><div><strong>{{ root.name }}</strong><small>{{ root.path }}</small><span v-if="root.diskLabel" class="tag">{{ root.diskLabel }}</span></div><button @click="removeRoot(root.id)">移除</button></div><div v-if="!roots.length" class="empty">暂无根目录 · 示例：S:\zeogit-ref</div></div>
        <div class="settings-actions">
          <button class="settings-scan-btn" :disabled="isBusy || !roots.length" @click="void triggerScan()">{{ scanning ? '扫描中…' : `扫描索引（${roots.length} 个根目录）` }}</button>
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
          <select v-model="policyRepoId" aria-label="选择仓库">
            <option :value="null">选择仓库…</option>
            <option v-for="repo in repos" :key="repo.id" :value="repo.id">{{ repo.org }}/{{ repo.name }}</option>
          </select>
          <button @click="policyRepoId!==null && loadPolicy(policyRepoId)">加载</button>
        </div>
        <div v-if="policyRepoId!==null" class="policy-form" style="display:grid;gap:8px;max-width:480px;">
          <label>拉取策略 <select v-model="policyForm.pullStrategy"><option value="fetch-only">fetch-only</option><option value="mirror">mirror</option><option value="no-update">no-update</option><option value="archive">archive</option></select></label>
          <label>冲突策略 <select v-model="policyForm.conflict"><option value="stop">stop</option><option value="backup">backup</option><option value="overwrite">overwrite</option><option value="abort">abort</option></select></label>
          <label>无人值守 <select v-model="policyForm.unattended"><option value="stop">stop</option><option value="backup">backup</option><option value="overwrite">overwrite</option><option value="abort">abort</option></select></label>
          <button class="settings-scan-btn" @click="savePolicy">保存策略</button>
        </div>
      </template>
    </main>
    <div v-if="selected" class="drawer-backdrop" @click="selected=null"></div>
    <aside v-if="selected" class="drawer"><button class="close" aria-label="关闭详情" @click="selected = null">×</button><p class="eyebrow">REPOSITORY DETAIL</p><h2>{{ selected.org }}/{{ selected.name }}</h2><small class="path" :title="selected.relPath">{{ selected.relPath }}<button class="copy-btn" @click="copyText(selected.relPath)" title="复制路径">⎘</button></small><dl><dt>类型</dt><dd><select class="kind-select" :value="selected.repoKind" @change="updateKind(($event.target as HTMLSelectElement).value)"><option value="third-party">third-party</option><option value="third-party-frozen">third-party-frozen</option><option value="fork">fork</option><option value="own">own</option></select><span v-if="selected.modifyLock" class="muted"> · 星枢修改锁</span></dd><dt>分支</dt><dd>{{ selected.defaultBranch ?? 'bare / unknown' }}</dd><dt>HEAD</dt><dd class="mono" :title="selected.headCommit ?? ''">{{ selected.headCommit?.slice(0, 10) ?? '—' }}<button v-if="selected.headCommit" class="copy-btn" @click="copyText(selected.headCommit!)" title="复制 HEAD">⎘</button></dd><dt>远程</dt><dd class="path" :title="selected.remoteUrl ?? ''">{{ selected.remoteUrl ?? '—' }}<button v-if="selected.remoteUrl" class="copy-btn" @click="copyText(selected.remoteUrl!)" title="复制远程">⎘</button></dd><dt>大小</dt><dd>{{ formatBytes(selected.sizeBytes) }}</dd><dt>标签</dt><dd><span v-for="slug in selected.tags" :key="slug" class="tag">{{ slug }}</span><span v-if="!selected.tags.length" class="muted">无</span></dd></dl><div style="margin-top:16px;display:flex;gap:6px;align-items:center;">
          <select v-model="moveTarget" aria-label="目标根"><option :value="null">移动至…</option><option v-for="root in roots" :key="root.id" :value="root.id" :disabled="root.id===selected.rootId">{{ root.name }} ({{ root.path }})</option></select>
          <button @click="moveRepo" :disabled="moveTarget===null">搬迁</button>
        </div>
        <details style="margin-top:12px;"><summary @click="selected && loadFetchLogs(selected.id)">拉取历史</summary><div v-if="fetchLogs.length"><div v-for="log in fetchLogs" :key="log.id" class="mono" style="font-size:11px;">{{ log.startedAt.slice(0,19) }} {{ log.strategy }} {{ labelResult(log.result) }}</div></div><div v-else class="muted">暂无历史</div></details>
        <div class="drawer-actions"><button @click="void pull(selected)">pull</button><select aria-label="添加标签" @change="selectTag($event, selected)"><option value="">添加标签…</option><option v-for="tag in tags" :key="tag.id" :value="tag.slug">{{ tag.label }}</option></select></div><div v-if="selected.lockViolation" class="conflict"><b>检测到本地改动</b><p>当前仓库被标记为锁定类型。选择处理方式：</p><button :disabled="!!resolvingAction" @click="void resolveConflict('backup')">{{ resolvingAction==='backup' ? '处理中…' : '备份后拉取' }}</button><button :disabled="!!resolvingAction" @click="void resolveConflict('overwrite')">{{ resolvingAction==='overwrite' ? '处理中…' : '覆盖本地' }}</button><button :disabled="!!resolvingAction" @click="void resolveConflict('abort')">{{ resolvingAction==='abort' ? '处理中…' : '保持现状' }}</button><p v-if="resolvingAction" class="muted" style="margin-top:8px">正在执行 {{ labelResult(resolvingAction) }}，大仓库可能耗时数秒…</p></div></aside>
  </div>
</template>
