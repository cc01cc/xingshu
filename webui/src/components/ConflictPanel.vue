<script setup lang="ts">
import { Button } from "@/components/ui/button";
import { FolderOpen } from "@lucide/vue";
import type { TaskConflict } from "../task-store";
import { conflictHeadline, conflictOutcomeLines, formatDuration, labelResult, labelStatus, shortBackupPath } from "../lib/format";

defineProps<{
  conflicts: TaskConflict[];
  overwriteConfirm: { scope: "task"; repoId: number } | null;
}>();

const emit = defineEmits<{
  decide: [repoId: number, action: "backup" | "overwrite" | "skip" | "abort"];
  requestOverwrite: [repoId: number];
  cancelOverwrite: [];
  openDir: [path: string];
  viewDiff: [repoId: number, path: string];
}>();
</script>

<template>
  <div class="task-conflicts">
    <div class="task-conflicts-heading"><strong>待决策仓库</strong><span>{{ conflicts.length }} 项</span></div>
    <article v-for="conflict in conflicts" :key="conflict.repoId" class="task-conflict">
      <div><strong>{{ conflict.repoName }}</strong><small class="path">{{ conflict.repoPath }}</small><small v-if="conflict.backupPath" class="path" :title="String(conflict.backupPath)">备份: {{ shortBackupPath(conflict.backupPath) }}<Button v-if="conflict.backupPath" variant="ghost" size="icon-sm" class="copy-btn" @click="emit('openDir', String(conflict.backupPath))" title="打开备份所在目录"><FolderOpen class="size-4" /></Button></small></div>
      <template v-if="conflict.conflictDetail">
        <p><b>{{ conflictHeadline(conflict.conflictDetail) }}</b> — {{ conflict.conflictDetail.reason }}</p>
        <div v-if="conflict.conflictDetail.kind === 'dirty'" class="conflict-detail">
          <p class="conflict-overview">已暂存 {{ conflict.conflictDetail.staged }} · 已修改 {{ conflict.conflictDetail.modified }} · 未跟踪 {{ conflict.conflictDetail.untracked }}（共 {{ conflict.conflictDetail.statusEntries.length }} 个文件）</p>
          <ul class="conflict-files">
            <li v-for="entry in conflict.conflictDetail.statusEntries.slice(0, 10)" :key="entry.path" class="mono diff-entry" :title="`${entry.path} · 点击查看只读 diff`" role="button" tabindex="0" @click="emit('viewDiff', conflict.repoId, entry.path)" @keydown.enter="emit('viewDiff', conflict.repoId, entry.path)"><span class="status-code">{{ entry.index.trim() || '-' }}{{ entry.worktree.trim() || '-' }}</span> {{ entry.path }}</li>
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
        <Button size="sm" :disabled="conflict.status !== 'waiting_decision'" @click="emit('decide', conflict.repoId, 'backup')">备份后拉取</Button>
        <Button v-if="overwriteConfirm?.scope === 'task' && overwriteConfirm.repoId === conflict.repoId" size="sm" variant="destructive" :disabled="conflict.status !== 'waiting_decision'" @click="emit('decide', conflict.repoId, 'overwrite'); emit('cancelOverwrite')">确认覆盖（丢弃本地变更，不可恢复）</Button>
        <Button v-else size="sm" variant="outline" :disabled="conflict.status !== 'waiting_decision'" @click="emit('requestOverwrite', conflict.repoId)">覆盖本地</Button>
        <Button size="sm" variant="ghost" :disabled="conflict.status !== 'waiting_decision'" @click="emit('decide', conflict.repoId, 'skip')">保持现状</Button>
      </div>
    </article>
  </div>
</template>
