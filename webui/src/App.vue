<script setup lang="ts">
import { computed, onMounted, ref } from "vue";

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
const query = ref("");
const tagFilter = ref("");
const view = ref<"repos" | "tags" | "disks" | "settings">("repos");
const selected = ref<Repo | null>(null);
const newTag = ref("");
const newRootPath = ref("");
const loading = ref(true);
const scanning = ref(false);
const error = ref("");

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
    error.value = reason instanceof Error ? reason.message : "无法加载索引";
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
  } catch (reason) { error.value = reason instanceof Error ? reason.message : "pull 失败"; }
}

async function resolveConflict(action: "backup" | "overwrite" | "abort"): Promise<void> {
  if (!selected.value) return;
  try {
    await request(`/api/v1/repos/${selected.value.id}/pull/${action}`, { method: "POST" });
    selected.value = null;
    await load();
  } catch (reason) { error.value = reason instanceof Error ? reason.message : "冲突处理失败"; }
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
  } catch (reason) { error.value = reason instanceof Error ? reason.message : "添加根目录失败"; }
}

async function removeRoot(id: number): Promise<void> {
  try {
    await request(`/api/v1/roots/${id}`, { method: "DELETE" });
    await load();
  } catch (reason) { error.value = reason instanceof Error ? reason.message : "删除根目录失败"; }
}

async function updateKind(kind: string): Promise<void> {
  if (!selected.value) return;
  try {
    await request(`/api/v1/repos/${selected.value.id}`, { method: "PUT", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ kind }) });
    await load();
    selected.value = repos.value.find((item) => item.id === selected.value?.id) ?? null;
  } catch (reason) { error.value = reason instanceof Error ? reason.message : "更新类型失败"; }
}

async function deleteTag(id: number): Promise<void> {
  try {
    await request(`/api/v1/tags/${id}`, { method: "DELETE" });
    await load();
  } catch (reason) { error.value = reason instanceof Error ? reason.message : "删除标签失败"; }
}

async function triggerScan(): Promise<void> {
  scanning.value = true;
  error.value = "";
  try {
    await request("/api/v1/scan", { method: "POST" });
    await load();
  } catch (reason) {
    error.value = reason instanceof Error ? reason.message : "扫描失败";
  } finally { scanning.value = false; }
}

function setView(nextView: "repos" | "tags" | "disks" | "settings"): void {
  view.value = nextView;
}

onMounted(() => void load());
</script>

<template>
  <div class="shell">
    <aside class="sidebar">
      <div class="brand"><span class="brand-mark">✦</span><span><strong>星枢</strong><small>XINGSHU</small></span></div>
      <nav>
        <a :class="{ active: view === 'repos' }" href="#" @click.prevent="setView('repos')">仓库 <span>{{ repos.length }}</span></a>
        <a :class="{ active: view === 'tags' }" href="#" @click.prevent="setView('tags')">标签 <span>{{ tags.length }}</span></a>
        <a :class="{ active: view === 'disks' }" href="#" @click.prevent="setView('disks')">磁盘看板</a>
        <a :class="{ active: view === 'settings' }" href="#" @click.prevent="setView('settings')">设置</a>
        <a v-if="conflicts.length" class="attention" href="#" @click.prevent="setView('repos')">待决策 <span>{{ conflicts.length }}</span></a>
        <button class="scan-btn" :disabled="scanning" @click="triggerScan">{{ scanning ? '扫描中…' : '扫描' }}</button>
      </nav>
      <div class="sidebar-note">本地仓库索引<br /><small>localhost only</small></div>
    </aside>
    <main class="main">
      <header class="toolbar">
        <div><p class="eyebrow">REPOSITORY INDEX</p><h1>{{ view === 'repos' ? '仓库目录' : view === 'tags' ? '主题标签' : '磁盘看板' }}</h1></div>
        <input v-if="view === 'repos'" v-model="query" aria-label="搜索仓库" placeholder="搜索仓库、组织或路径…" />
      </header>
      <p v-if="error" class="notice error">{{ error }}</p>
      <div v-else-if="loading" class="empty">正在读取索引…</div>
      <template v-else-if="view === 'repos'">
        <section class="summary"><span><b>{{ filteredRepos.length }}</b> 个结果</span><select v-model="tagFilter" aria-label="按标签过滤"><option value="">全部标签</option><option v-for="tag in tags" :key="tag.id" :value="tag.slug">{{ tag.label }}</option></select><span class="legend"><i class="dot safe" />索引正常 <i class="dot warn" />需要处理</span></section>
        <div v-if="filteredRepos.length === 0" class="empty">没有匹配的仓库</div>
        <div v-else class="table-wrap"><table><thead><tr><th>仓库</th><th>类型</th><th>标签</th><th>大小</th><th>状态</th><th /></tr></thead><tbody><tr v-for="repo in filteredRepos" :key="repo.id" @click="selected = repo">
          <td><strong>{{ repo.org }}/{{ repo.name }}</strong><small>{{ repo.relPath }}</small></td><td><span class="kind" :class="repo.repoKind">{{ repo.repoKind }}</span></td><td><span v-for="slug in repo.tags" :key="slug" class="tag">{{ slug }}</span><span v-if="!repo.tags.length" class="muted">未分类</span></td><td class="mono">{{ formatBytes(repo.sizeBytes) }}</td><td><span v-if="repo.lockViolation" class="status warning">本地改动</span><span v-else class="status">{{ repo.lastPullStatus ?? "未更新" }}</span></td><td><button class="more" @click.stop="selected = repo">···</button></td>
        </tr></tbody></table></div>
      </template>
      <section v-else-if="view === 'tags'" class="tag-grid"><form class="tag-create" @submit.prevent="void createTag()"><input v-model="newTag" aria-label="新标签" placeholder="新建主题标签…" /><button type="submit">添加</button></form><div v-for="tag in tags" :key="tag.id" class="tag-card"><button class="tag-delete" @click.stop="deleteTag(tag.id)" aria-label="删除标签">×</button><button class="tag-card-body" @click="filterByTag(tag.slug)"><b>{{ tag.label }}</b><small>{{ repos.filter((repo) => repo.tags.includes(tag.slug)).length }} 个仓库</small></button></div></section>
      <template v-else-if="view === 'disks'">
        <section class="dashboard"><div class="stat-card"><small>索引仓库</small><strong>{{ stats.repositories }}</strong></div><div class="stat-card"><small>已占用空间</small><strong>{{ formatBytes(stats.bytes) }}</strong></div><div class="stat-card"><small>待决策</small><strong class="accent">{{ conflicts.length }}</strong></div></section>
        <div v-if="Object.keys(stats.byKind).length" class="kind-bar"><div v-for="(count, kind) in stats.byKind" :key="kind" class="kind-segment" :style="{ flex: count }"><span class="kind" :class="kind">{{ kind }}</span> {{ count }}</div></div>
      </template>
      <template v-else-if="view === 'settings'">
        <h2 class="settings-title">根目录管理</h2>
        <form class="root-create" @submit.prevent="addRoot"><input v-model="newRootPath" aria-label="根目录路径" placeholder="输入根目录绝对路径…" /><button type="submit">添加</button></form>
        <div class="root-list"><div v-for="root in roots" :key="root.id" class="root-item"><div><strong>{{ root.name }}</strong><small>{{ root.path }}</small><span v-if="root.diskLabel" class="tag">{{ root.diskLabel }}</span></div><button @click="removeRoot(root.id)">移除</button></div><div v-if="!roots.length" class="empty">暂无根目录</div></div>
        <h2 class="settings-title">关于</h2>
        <dl class="about"><dt>版本</dt><dd>0.1.0</dd><dt>API 端口</dt><dd>12681</dd><dt>WebUI 端口</dt><dd>12680</dd></dl>
      </template>
    </main>
    <aside v-if="selected" class="drawer"><button class="close" aria-label="关闭详情" @click="selected = null">×</button><p class="eyebrow">REPOSITORY DETAIL</p><h2>{{ selected.org }}/{{ selected.name }}</h2><small class="path">{{ selected.relPath }}</small><dl><dt>类型</dt><dd><select class="kind-select" :value="selected.repoKind" @change="updateKind(($event.target as HTMLSelectElement).value)"><option value="third-party">third-party</option><option value="third-party-frozen">third-party-frozen</option><option value="fork">fork</option><option value="own">own</option></select><span v-if="selected.modifyLock" class="muted"> · 星枢修改锁</span></dd><dt>分支</dt><dd>{{ selected.defaultBranch ?? 'bare / unknown' }}</dd><dt>HEAD</dt><dd class="mono">{{ selected.headCommit?.slice(0, 10) ?? '—' }}</dd><dt>远程</dt><dd class="path">{{ selected.remoteUrl ?? '—' }}</dd><dt>大小</dt><dd>{{ formatBytes(selected.sizeBytes) }}</dd><dt>标签</dt><dd><span v-for="slug in selected.tags" :key="slug" class="tag">{{ slug }}</span><span v-if="!selected.tags.length" class="muted">无</span></dd></dl><div class="drawer-actions"><button @click="void pull(selected)">pull</button><select aria-label="添加标签" @change="selectTag($event, selected)"><option value="">添加标签…</option><option v-for="tag in tags" :key="tag.id" :value="tag.slug">{{ tag.label }}</option></select></div><div v-if="selected.lockViolation" class="conflict"><b>检测到本地改动</b><p>当前仓库被标记为锁定类型。选择处理方式：</p><button @click="void resolveConflict('backup')">备份后拉取</button><button @click="void resolveConflict('overwrite')">覆盖本地</button><button @click="void resolveConflict('abort')">保持现状</button></div></aside>
  </div>
</template>
