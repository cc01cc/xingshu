<script setup lang="ts">
import { computed, ref } from "vue";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { useTaskStore } from "../task-store";
import { badgeClassForResult, formatDuration, labelResult, labelStatus, shortBackupPath } from "../lib/format";
import ConflictPanel from "./ConflictPanel.vue";
import DiffDialog from "./DiffDialog.vue";

defineProps<{
  overwriteConfirm: { scope: "task"; repoId: number } | null;
}>();

const emit = defineEmits<{
  dismiss: [];
  decide: [repoId: number, action: "backup" | "overwrite" | "skip" | "abort"];
  requestOverwrite: [repoId: number];
  cancelOverwrite: [];
  openDir: [path: string];
  gotoRepos: [];
}>();

const taskStore = useTaskStore();
const { task, taskConflicts, connectionLabel, error: taskError, progressPercent, isBusy } = taskStore;
const diffTarget = ref<{ repoId: number; path: string } | null>(null);

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
const taskTotal = computed(() => {
  if (!taskCounts.value) return null;
  return Object.values(taskCounts.value).reduce((sum, n) => sum + (Number(n) || 0), 0);
});
const taskSummaryText = computed(() => {
  if (!taskCounts.value || taskTotal.value === null) return "";
  const entries = Object.entries(taskCounts.value).map(([k, v]) => `${labelResult(k)} ${v}`);
  return `共 ${taskTotal.value} 个仓库：${entries.join("、")}`;
});
</script>

<template>
  <section v-if="task" class="task-panel" aria-live="polite">
    <div class="task-heading"><div><p class="eyebrow">ASYNC TASK</p><strong>{{ task.type === 'scan' ? '索引扫描' : '批量 pull' }}</strong><Badge variant="outline" class="task-status" :class="task.status">{{ labelStatus(task.status) }}</Badge></div><Button v-if="!isBusy" variant="ghost" size="sm" @click="emit('dismiss')">关闭</Button></div>
    <div class="task-meta"><span>{{ connectionLabel }}</span><span>{{ taskProgressLabel }}</span><span v-if="task.lastRepo" class="mono" :title="task.lastRepo">{{ task.lastRepo }}</span></div>
    <div class="task-progress" :class="{ indeterminate: isBusy && progressPercent === null }"><span :style="progressPercent === null ? (task?.status === 'completed' ? { width: '100%' } : isBusy ? undefined : { width: '100%', opacity: '0.5' }) : { width: `${progressPercent}%` }" /></div>
    <p v-if="task.lastResult" class="task-result">{{ labelStatus(task.lastResult) }}</p>
    <ConflictPanel v-if="taskConflicts.length" :conflicts="taskConflicts" :overwrite-confirm="overwriteConfirm" @decide="(repoId, action) => emit('decide', repoId, action)" @request-overwrite="(repoId) => emit('requestOverwrite', repoId)" @cancel-overwrite="emit('cancelOverwrite')" @open-dir="(path) => emit('openDir', path)" @view-diff="(repoId, path) => (diffTarget = { repoId, path })" />
    <DiffDialog :open="diffTarget !== null" :repo-id="diffTarget?.repoId ?? null" :path="diffTarget?.path ?? null" @close="diffTarget = null" />
    <button v-if="taskHasConflict" class="task-link" @click="emit('gotoRepos')">查看冲突仓</button>
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
</template>
