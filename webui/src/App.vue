<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { Menu } from "@lucide/vue";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { Sheet } from "@/components/ui/sheet";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useTaskStore, type TaskType } from "./task-store";
import { friendlyError, request } from "./lib/api";
import type { Repo, Root, Stats, Tag } from "./lib/types";
import RepoTable from "./components/RepoTable.vue";
import TaskPanel from "./components/TaskPanel.vue";
import RepoDrawer from "./components/RepoDrawer.vue";
import AppSidebar from "./components/AppSidebar.vue";
import TagsView from "./views/TagsView.vue";
import DisksView from "./views/DisksView.vue";
import SettingsView from "./views/SettingsView.vue";
import PolicyView from "./views/PolicyView.vue";

const repos = ref<Repo[]>([]);
const tags = ref<Tag[]>([]);
const roots = ref<Root[]>([]);
const stats = ref<Stats>({ bytes: 0, repositories: 0, byKind: {} });
const query = ref("");
const tagFilter = ref("");
const view = ref<"repos" | "tags" | "disks" | "settings" | "policy">("repos");
const selected = ref<Repo | null>(null);
const mobileNavOpen = ref(false);
const copyFeedback = ref("");
const loading = ref(true);
const error = ref("");
const taskStore = useTaskStore();
const { task, isBusy } = taskStore;
const scanning = computed(() => isBusy.value && task.value?.type === "scan");
const pulling = computed(() => isBusy.value && task.value?.type === "pull");
const overwriteConfirm = ref<null | { scope: "task"; repoId: number }>(null);

async function openDirectory(path: string): Promise<void> {
  try {
    await request("/api/v1/open", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ path }) });
  } catch (reason) {
    error.value = friendlyError(reason instanceof Error ? reason.message : "打开目录失败");
  }
}
function requestOverwriteConfirm(scope: "task", repoId: number): void {
  overwriteConfirm.value = { scope, repoId };
}
function cancelOverwrite(): void {
  overwriteConfirm.value = null;
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

const filteredRepos = computed(() => repos.value.filter((repo) => {
  const needle = query.value.trim().toLowerCase();
  const matchesQuery = !needle || `${repo.org}/${repo.name} ${repo.relPath}`.toLowerCase().includes(needle);
  const matchesTag = !tagFilter.value || repo.tags.includes(tagFilter.value);
  const status = statusFilter.value;
  const matchesStatus = status === "__all"
    || (status === "dirty" && repo.dirty)
    || (status === "ahead" && repo.ahead > 0)
    || (status === "behind" && repo.behind > 0);
  return matchesQuery && matchesTag && matchesStatus;
}));
const conflicts = computed(() => repos.value.filter((repo) => repo.lockViolation || repo.lastPullStatus === "conflict"));

const statusFilter = ref("__all");

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
    if (selected.value) selected.value = repos.value.find((item) => item.id === selected.value!.id) ?? selected.value;
    error.value = "";
  } catch (reason) {
    const message = reason instanceof Error ? reason.message : "无法加载索引";
    error.value = friendlyError(message);
  } finally { loading.value = false; }
}

async function createTag(slug: string): Promise<void> {
  if (!slug) return;
  await request("/api/v1/tags", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ slug, label: slug }) });
  await load();
}

function filterByTag(slug: string): void {
  tagFilter.value = slug;
  view.value = "repos";
}

async function addRoot(path: string): Promise<void> {
  if (!path) return;
  try {
    await request("/api/v1/roots", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ path }) });
    await load();
  } catch (reason) { error.value = friendlyError(reason instanceof Error ? reason.message : "添加根目录失败"); }
}

async function removeRoot(id: number): Promise<void> {
  try {
    await request(`/api/v1/roots/${id}`, { method: "DELETE" });
    await load();
  } catch (reason) { error.value = friendlyError(reason instanceof Error ? reason.message : "删除根目录失败"); }
}

async function deleteTag(id: number): Promise<void> {
  try {
    await request(`/api/v1/tags/${id}`, { method: "DELETE" });
    await load();
  } catch (reason) { error.value = friendlyError(reason instanceof Error ? reason.message : "删除标签失败"); }
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
  action: "backup" | "overwrite" | "skip" | "abort",
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
  <div class="shell style-nova">
    <AppSidebar :view="view" :repos-count="repos.length" :tags-count="tags.length" :conflicts-count="conflicts.length" :mobile-nav-open="mobileNavOpen" :is-busy="isBusy" :scanning="scanning" :pulling="pulling" @set-view="setView" @scan="void triggerScan()" @pull="void triggerPull()" @close-mobile-nav="closeMobileNav" />
    <main class="main">
      <header class="toolbar">
        <Button variant="ghost" size="icon" class="hamburger" aria-label="切换导航" @click="toggleMobileNav"><Menu class="size-4" /></Button>
        <div><p class="eyebrow">REPOSITORY INDEX</p><h1>{{ view === 'repos' ? '仓库目录' : view === 'tags' ? '主题标签' : view === 'disks' ? '磁盘看板' : view === 'policy' ? '策略' : '设置' }}</h1></div>
        <Input v-if="view === 'repos'" v-model="query" aria-label="搜索仓库" placeholder="搜索仓库、组织或路径…" />
        <span v-if="copyFeedback" class="copy-feedback" style="color:#d6aa68;font-size:11px;margin-left:8px;">{{ copyFeedback }}</span>
      </header>
        <TaskPanel v-if="task" :overwrite-confirm="overwriteConfirm" @dismiss="taskStore.dismiss" @decide="(repoId, action) => void decideTaskConflict(repoId, action)" @request-overwrite="requestOverwriteConfirm('task', $event)" @cancel-overwrite="cancelOverwrite" @open-dir="(path) => void openDirectory(path)" @goto-repos="setView('repos')" />
        <p v-if="error" class="notice error">{{ error }}</p>
      <div v-else-if="loading">
          <Skeleton style="height:16px;width:40%;margin-top:42px;" />
          <Skeleton style="height:48px;margin-top:12px;" />
          <Skeleton style="height:48px;margin-top:8px;" />
          <Skeleton style="height:48px;margin-top:8px;" />
        </div>
      <template v-else-if="view === 'repos'">
        <section class="summary"><span><b>{{ filteredRepos.length }}</b> 个结果</span><Select :model-value="tagFilter || '__all'" @update:model-value="tagFilter = $event === '__all' ? '' : String($event ?? '')"><SelectTrigger class="w-44" aria-label="按标签过滤"><SelectValue placeholder="全部标签" /></SelectTrigger><SelectContent><SelectItem value="__all">全部标签</SelectItem><SelectItem v-for="tag in tags" :key="tag.id" :value="tag.slug">{{ tag.label }}</SelectItem></SelectContent></Select><Select :model-value="statusFilter" @update:model-value="statusFilter = String($event ?? '__all')"><SelectTrigger class="w-44" aria-label="按状态过滤"><SelectValue placeholder="全部状态" /></SelectTrigger><SelectContent><SelectItem value="__all">全部状态</SelectItem><SelectItem value="dirty">本地改动</SelectItem><SelectItem value="ahead">领先</SelectItem><SelectItem value="behind">落后</SelectItem></SelectContent></Select><span class="legend"><i class="dot safe" />索引正常 <i class="dot warn" />需要处理</span></section>
        <div v-if="filteredRepos.length === 0" class="empty">
          <template v-if="!repos.length && roots.length">暂无仓库索引 · 请到 <a href="#" @click.prevent="setView('settings')">设置</a> 点击「扫描索引」</template>
          <template v-else-if="!repos.length && !roots.length">暂无根目录 · 请到 <a href="#" @click.prevent="setView('settings')">设置</a> 添加根目录后扫描</template>
          <template v-else>没有匹配的仓库</template>
        </div>
        <RepoTable v-else :repos="filteredRepos" :tags="tags" :selected="selected" @select="selected = $event" @copy="(path) => copyText(path)" />
      </template>
      <TagsView v-else-if="view === 'tags'" :tags="tags" :repos="repos" @create="createTag" @delete="deleteTag" @filter="filterByTag" />
      <DisksView v-else-if="view === 'disks'" :stats="stats" :conflicts="conflicts.length" :roots="roots" :repos="repos" />
      <SettingsView v-else-if="view === 'settings'" :roots="roots" :repos="repos" :is-busy="isBusy" :scanning="scanning" :has-task="task !== null" @add-root="addRoot" @remove-root="removeRoot" @scan="triggerScan" />
      <PolicyView v-else-if="view === 'policy'" :repos="repos" :tags="tags" @notice="error = $event" />
    </main>
    <Sheet :open="selected !== null" @update:open="(value: boolean) => { if (!value) selected = null; }">
      <RepoDrawer v-if="selected" :repo="selected" :roots="roots" :tags="tags" :reload="load" :notify="(message) => (error = message)" @close="selected = null" @copy="(text) => copyText(text)" />
    </Sheet>
  </div>
</template>
