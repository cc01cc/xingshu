<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { Copy, X } from "@lucide/vue";
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
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { SheetContent, SheetDescription, SheetHeader, SheetTitle } from "@/components/ui/sheet";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip";
import { friendlyError, request } from "../lib/api";
import { formatBackupTime, formatBytes, formatFetchTime, formatScanTime, kindChangeHint, labelResult } from "../lib/format";
import type { Repo, Root, Tag } from "../lib/types";

const props = defineProps<{
  repo: Repo;
  roots: Root[];
  tags: Tag[];
  reload: () => Promise<void>;
  notify: (message: string) => void;
}>();

const emit = defineEmits<{
  close: [];
  copy: [text: string];
}>();

const pendingKind = ref<string | null>(null);
const kindConfirmOpen = ref(false);
const fetchLogs = ref<Array<{ id: number; strategy: string; result: string; startedAt: string }>>([]);
const moveTarget = ref<number | null>(null);
const moveTargetSpace = ref<number | null>(null);
const moveSpaceError = ref<string | null>(null);
const moveSpaceLoading = ref(false);
const spaceInsufficient = computed(
  () => moveTargetSpace.value !== null && moveTargetSpace.value <= props.repo.sizeBytes,
);

watch(moveTarget, async (target) => {
  moveTargetSpace.value = null;
  moveSpaceError.value = null;
  if (target === null) return;
  moveSpaceLoading.value = true;
  try {
    const response = await request(`/api/v1/roots/${target}/space`);
    const data = (await response.json()) as { availableBytes: number };
    moveTargetSpace.value = data.availableBytes;
  } catch (reason) {
    moveSpaceError.value = friendlyError(reason instanceof Error ? reason.message : "目标空间探测失败");
  } finally {
    moveSpaceLoading.value = false;
  }
});
const drawerStatus = ref<{ dirty: boolean; ahead: number; behind: number; statusCheckedAt: string | null } | null>(null);
const remoteChecking = ref(false);
const remoteState = ref<string | null>(null);
const remoteDetail = ref<string | null>(null);
type BackupEntry = { path: string; isArchive: boolean; bytes: number; createdAt: string };
const backupEntries = ref<BackupEntry[]>([]);
const backupBusy = ref(false);
const restoreConfirmPath = ref<string | null>(null);
const prunePreviewEntries = ref<string[] | null>(null);
const pruneKeep = ref(5);
const pruneExecuting = ref(false);
const resolvingAction = ref<"backup" | "overwrite" | "skip" | "abort" | null>(null);
const overwriteConfirm = ref<{ repoId: number } | null>(null);

function repoAbsolutePath(): string | null {
  const root = props.roots.find((item) => item.id === props.repo.rootId);
  if (!root) return null;
  return `${root.path.replace(/[\\/]+$/, "")}/${props.repo.relPath}`;
}

async function openDirectory(path: string): Promise<void> {
  try {
    await request("/api/v1/open", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ path }) });
  } catch (reason) {
    props.notify(friendlyError(reason instanceof Error ? reason.message : "打开目录失败"));
  }
}

function requestKindChange(value: unknown): void {
  const next = String(value ?? "");
  if (!next || next === props.repo.repoKind) return;
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

async function updateKind(kind: string): Promise<void> {
  try {
    await request(`/api/v1/repos/${props.repo.id}`, { method: "PUT", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ kind }) });
    void props.reload();
  } catch (reason) {
    props.notify(friendlyError(reason instanceof Error ? reason.message : "更新类型失败"));
  }
}

async function attachTag(slug: string): Promise<void> {
  if (!slug || props.repo.tags.includes(slug)) return;
  await request(`/api/v1/repos/${props.repo.id}/tags`, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ slug }) });
  void props.reload();
}

async function pull(): Promise<void> {
  try {
    await request(`/api/v1/repos/${props.repo.id}/pull`, { method: "POST" });
    void props.reload();
  } catch (reason) {
    props.notify(friendlyError(reason instanceof Error ? reason.message : "pull 失败"));
  }
}

async function resolveConflict(action: "backup" | "overwrite" | "skip" | "abort"): Promise<void> {
  if (resolvingAction.value) return;
  resolvingAction.value = action;
  try {
    const response = await request(`/api/v1/repos/${props.repo.id}/pull/${action}`, { method: "POST" });
    try {
      const body: unknown = await response.clone().json();
      if (body && typeof body === "object" && "backupPath" in (body as Record<string, unknown>)) {
        const bp = (body as Record<string, unknown>).backupPath;
        if (bp) console.info("backup created", bp);
      }
    } catch { /* ignore json parse */ }
    emit("close");
    void props.reload();
  } catch (reason) {
    const message = reason instanceof Error ? reason.message : "冲突处理失败";
    props.notify(friendlyError(message));
  } finally {
    resolvingAction.value = null;
  }
}

async function loadFetchLogs(): Promise<void> {
  try {
    const r = await request(`/api/v1/repos/${props.repo.id}/fetch_log`);
    const j = await r.json() as Array<Record<string, unknown>>;
    fetchLogs.value = j.map((x) => ({ id: Number(x.id), strategy: String(x.strategy ?? ""), result: String(x.result ?? ""), startedAt: String(x.startedAt ?? x.started_at ?? "") }));
  } catch { fetchLogs.value = []; }
}

async function checkRemote(): Promise<void> {
  remoteChecking.value = true;
  try {
    const response = await request(`/api/v1/repos/${props.repo.id}/status?remote=true`);
    if (!response.ok) { remoteState.value = "unknown（服务请求失败）"; return; }
    const data = await response.json();
    drawerStatus.value = { dirty: data.dirty, ahead: data.ahead, behind: data.behind, statusCheckedAt: data.statusCheckedAt };
    const remote = data.remote as null | { state: string; detail?: string };
    remoteDetail.value = remote?.detail ?? null;
    remoteState.value = !remote ? ""
      : remote.state === "up-to-date" ? "远端无变化"
      : remote.state === "changed" ? "远端有更新：远端分支已移动，需要 fetch/pull"
      : `unknown：无法连接 origin`;
  } catch { remoteState.value = "unknown（网络或服务不可达）"; }
  finally { remoteChecking.value = false; }
}

async function loadBackups(): Promise<void> {
  try {
    const response = await request(`/api/v1/repos/${props.repo.id}/backups`);
    if (!response.ok) return;
    backupEntries.value = await response.json();
  } catch { /* 状态栏已有错误提示通道，列表失败静默保留旧值 */ }
}

async function createRepoBackup(): Promise<void> {
  backupBusy.value = true;
  try {
    const response = await request(`/api/v1/repos/${props.repo.id}/backups`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ keep: 5 }),
    });
    if (response.ok) await loadBackups();
  } finally { backupBusy.value = false; }
}

async function restoreRepoBackup(entry: BackupEntry): Promise<void> {
  if (restoreConfirmPath.value !== entry.path) { restoreConfirmPath.value = entry.path; return; }
  restoreConfirmPath.value = null;
  backupBusy.value = true;
  try {
    const response = await request(`/api/v1/repos/${props.repo.id}/backups/restore`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ path: entry.path, confirm: true }),
    });
    if (response.ok) {
      await loadBackups();
      const statusResponse = await fetch(`/api/v1/repos/${props.repo.id}/status`);
      if (statusResponse.ok) {
        const data = await statusResponse.json();
        drawerStatus.value = { dirty: data.dirty, ahead: data.ahead, behind: data.behind, statusCheckedAt: data.statusCheckedAt };
      }
    }
  } finally { backupBusy.value = false; }
}

function prunePreviewList(): void {
  pruneKeep.value = Math.max(0, pruneKeep.value);
  backupBusy.value = true;
  fetch(`/api/v1/repos/${props.repo.id}/backups/prune`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ keep: pruneKeep.value, dryRun: true }),
  })
    .then((response) => response.json())
    .then((data) => { prunePreviewEntries.value = data.affected ?? []; })
    .catch(() => { prunePreviewEntries.value = []; })
    .finally(() => { backupBusy.value = false; });
}

async function executePrune(): Promise<void> {
  if (prunePreviewEntries.value === null) return;
  pruneExecuting.value = true;
  try {
    await request(`/api/v1/repos/${props.repo.id}/backups/prune`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ keep: pruneKeep.value, dryRun: false }),
    });
    prunePreviewEntries.value = null;
    await loadBackups();
  } finally { pruneExecuting.value = false; }
}

async function moveRepo(): Promise<void> {
  if (moveTarget.value === null) return;
  try {
    await request(`/api/v1/repos/${props.repo.id}/move`, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ targetRootId: moveTarget.value }) });
    void props.reload();
    props.notify("搬迁成功");
  } catch (reason) {
    props.notify(friendlyError(reason instanceof Error ? reason.message : "搬迁失败"));
  }
}
</script>

<template>
  <TooltipProvider>
    <SheetContent class="drawer" :show-close-button="false">
    <Button variant="ghost" size="icon" class="close" aria-label="关闭详情" @click="emit('close')"><X class="size-4" /></Button>
    <SheetHeader class="drawer-head"><p class="eyebrow">REPOSITORY DETAIL</p><SheetTitle class="drawer-title">{{ repo.org }}/{{ repo.name }}</SheetTitle><SheetDescription class="sr-only">仓库详情与操作</SheetDescription></SheetHeader>
    <small class="path" :title="repo.relPath">{{ repo.relPath }}<Button variant="ghost" size="icon-sm" class="copy-btn" @click="emit('copy', repo.relPath)" title="复制路径"><Copy class="size-4" /></Button></small>
    <dl>
      <dt>类型</dt>
      <dd>
        <Select :model-value="repo.repoKind" @update:model-value="requestKindChange($event)">
          <SelectTrigger class="w-44" aria-label="仓库类型"><SelectValue /></SelectTrigger>
          <SelectContent>
            <SelectItem value="third-party">third-party</SelectItem>
            <SelectItem value="third-party-frozen">third-party-frozen</SelectItem>
            <SelectItem value="fork">fork</SelectItem>
            <SelectItem value="own">own</SelectItem>
          </SelectContent>
        </Select>
        <AlertDialog v-model:open="kindConfirmOpen">
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>切换仓库类型？</AlertDialogTitle>
              <AlertDialogDescription>{{ pendingKind ? kindChangeHint(pendingKind) : '' }}该操作立即写入索引。</AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel>取消</AlertDialogCancel>
              <AlertDialogAction @click="void confirmKindChange()">确认切换</AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
        <span v-if="repo.modifyLock" class="muted"> · 只读约定<Tooltip>
          <TooltipTrigger as-child><span class="lock-hint" tabindex="0" aria-label="只读约定说明"> ?</span></TooltipTrigger>
          <TooltipContent class="max-w-60">third-party 约定只读：出现本地改动会被标为待决策，pull 时需人工确认；不阻止你在文件管理器里直接修改。</TooltipContent>
        </Tooltip></span>
      </dd>
      <dt>分支</dt><dd>{{ repo.defaultBranch ?? 'bare / unknown' }}</dd>
      <dt>HEAD</dt><dd class="mono" :title="repo.headCommit ?? ''">{{ repo.headCommit?.slice(0, 10) ?? '—' }}<Button v-if="repo.headCommit" variant="ghost" size="icon-sm" class="copy-btn" @click="emit('copy', repo.headCommit!)" title="复制 HEAD"><Copy class="size-4" /></Button></dd>
      <dt>远程</dt><dd class="path" :title="repo.remoteUrl ?? ''">{{ repo.remoteUrl ?? '—' }}<Button v-if="repo.remoteUrl" variant="ghost" size="icon-sm" class="copy-btn" @click="emit('copy', repo.remoteUrl!)" title="复制远程"><Copy class="size-4" /></Button></dd>
      <dt>大小</dt><dd>{{ formatBytes(repo.sizeBytes) }}</dd>
      <dt>标签</dt><dd><Badge v-for="slug in repo.tags" :key="slug" variant="secondary">{{ slug }}</Badge><span v-if="!repo.tags.length" class="muted">无</span></dd>
    </dl>
    <div style="margin-top:16px;display:flex;gap:6px;align-items:center;">
      <Select :model-value="moveTarget === null ? undefined : String(moveTarget)" @update:model-value="moveTarget = $event == null || $event === '' ? null : Number($event)">
        <SelectTrigger class="min-w-0 flex-1" aria-label="目标根"><SelectValue placeholder="移动至…" /></SelectTrigger>
        <SelectContent>
          <SelectItem v-for="root in roots" :key="root.id" :value="String(root.id)" :disabled="root.id===repo.rootId">{{ root.name }} ({{ root.path }})</SelectItem>
        </SelectContent>
      </Select>
      <Button size="sm" @click="void moveRepo()" :disabled="moveTarget===null || moveSpaceLoading || spaceInsufficient">搬迁</Button>
    </div>
    <p v-if="moveTarget !== null" class="move-space" :class="{ 'space-short': spaceInsufficient }">
      <template v-if="moveSpaceLoading">正在探测目标空间…</template>
      <template v-else-if="moveSpaceError">{{ moveSpaceError }}</template>
      <template v-else-if="moveTargetSpace !== null">目标可用 {{ formatBytes(moveTargetSpace) }} · 需要 {{ formatBytes(repo.sizeBytes) }}<span v-if="spaceInsufficient"> · 空间不足，搬迁已禁用</span></template>
    </p>
    <details class="fetch-log"><summary @click="void loadFetchLogs()">拉取历史</summary><div v-if="fetchLogs.length" class="fetch-log-list"><div v-for="log in fetchLogs" :key="log.id" class="fetch-log-row"><span class="mono">{{ formatFetchTime(log.startedAt) }}</span><span class="mono">{{ log.strategy }}</span><Badge variant="outline">{{ labelResult(log.result) }}</Badge></div></div><div v-else class="muted">暂无历史</div></details>
    <div class="section-status">
      <div class="section-title-row"><b>状态</b><span class="muted">扫描于 {{ formatScanTime(drawerStatus) }}</span></div>
      <div class="kv-inline">
        <Badge v-if="drawerStatus?.dirty" variant="outline" class="status warning">本地改动</Badge>
        <Badge v-else-if="drawerStatus && drawerStatus.ahead > 0" variant="outline" class="status warning">领先 {{ drawerStatus.ahead }}</Badge>
        <Badge v-else variant="outline">工作区干净</Badge>
        <Badge v-if="drawerStatus && drawerStatus.behind > 0" variant="outline">落后 {{ drawerStatus.behind }}</Badge>
      </div>
      <div class="kv-inline" style="margin-top:10px">
        <Button size="sm" variant="outline" :disabled="remoteChecking" @click="void checkRemote()">{{ remoteChecking ? "检测中…" : "检测远端" }}</Button>
        <span class="muted">只读 ls-remote · 不更新本地引用</span>
      </div>
      <p v-if="remoteState" class="remote-result" :title="remoteDetail ?? undefined">{{ remoteState }}</p>
    </div>
    <div class="section-backup">
      <div class="section-title-row"><b>备份</b><Button size="sm" :disabled="backupBusy" @click="void createRepoBackup()">创建备份</Button></div>
      <div v-if="backupEntries.length">
        <div v-for="entry in backupEntries" :key="entry.path" class="backup-row">
          <span class="backup-time">{{ formatBackupTime(entry) }}</span>
          <Badge variant="outline">{{ entry.isArchive ? "归档" : "目录" }}</Badge>
          <span v-if="entry.isArchive" class="muted">{{ formatBytes(entry.bytes) }}</span>
          <span v-else class="muted">—</span>
          <span class="backup-ops">
            <Button size="sm" variant="outline" :disabled="backupBusy" @click="void restoreRepoBackup(entry)">{{ restoreConfirmPath === entry.path ? "确认还原" : "还原" }}</Button>
            <Button v-if="repoAbsolutePath()" size="sm" variant="ghost" @click="void openDirectory(repoAbsolutePath()!)">打开</Button>
          </span>
        </div>
        <Button size="sm" variant="destructive" class="prune-entry" :disabled="backupBusy" @click="prunePreviewEntries === null ? prunePreviewList() : void executePrune()">{{ prunePreviewEntries === null ? "清理…" : `确认清理（删除 ${prunePreviewEntries.length} 项）` }}</Button>
        <div v-if="prunePreviewEntries !== null" class="prune-preview">
          <div class="prune-keep"><span class="muted">保留数量 keep</span><input class="input prune-keep-input" type="number" min="0" :value="pruneKeep" @change="pruneKeep = Number(($event.target as HTMLInputElement).value || 0)" /><Button size="sm" variant="outline" :disabled="backupBusy" @click="void prunePreviewList()">重新预览</Button><Button size="sm" variant="ghost" @click="prunePreviewEntries = null">取消</Button></div>
          <div v-if="prunePreviewEntries.length === 0" class="muted">没有超出保留数量的备份，无需清理。</div>
          <div v-else>
            <div class="muted">将删除 {{ prunePreviewEntries.length }} 项（预览模式，未删除任何文件）：</div>
            <div v-for="path in prunePreviewEntries" :key="path" class="mono prune-path">{{ path }}</div>
          </div>
          <Button v-if="prunePreviewEntries.length > 0" size="sm" variant="destructive" :disabled="pruneExecuting" @click="void executePrune()">{{ pruneExecuting ? "清理中…" : "确认清理" }}</Button>
        </div>
      </div>
      <div v-else class="muted">暂无备份 · 「创建备份」生成目录快照或归档</div>
    </div>
    <div class="drawer-actions">
      <Button size="sm" @click="void pull()">pull</Button>
      <Button v-if="repoAbsolutePath()" size="sm" variant="outline" @click="void openDirectory(repoAbsolutePath()!)" title="在文件管理器中打开仓库目录">打开目录</Button>
      <Select @update:model-value="void attachTag(String($event ?? ''))">
        <SelectTrigger aria-label="添加标签"><SelectValue placeholder="添加标签…" /></SelectTrigger>
        <SelectContent>
          <SelectItem v-for="tag in tags" :key="tag.id" :value="tag.slug">{{ tag.label }}</SelectItem>
        </SelectContent>
      </Select>
    </div>
    <div v-if="repo.lockViolation" class="conflict">
      <b>检测到本地改动</b>
      <p>当前仓库被标记为锁定类型。选择处理方式：</p>
      <ul class="conflict-outcome-list"><li><b>备份后拉取</b>：整仓快照至 .bak.<时间戳> 后重新克隆，本地变更随备份保留</li><li><b>覆盖本地</b>：丢弃全部变更（含未跟踪文件），不可恢复</li><li><b>保持现状</b>：不做任何操作</li></ul>
      <Button :disabled="!!resolvingAction" @click="void resolveConflict('backup')">{{ resolvingAction==='backup' ? '处理中…' : '备份后拉取' }}</Button>
      <Button v-if="overwriteConfirm" variant="destructive" :disabled="!!resolvingAction" @click="void resolveConflict('overwrite'); overwriteConfirm = null">确认覆盖（丢弃本地变更，不可恢复）</Button>
      <Button v-else variant="outline" :disabled="!!resolvingAction" @click="overwriteConfirm = { repoId: repo.id }">{{ resolvingAction==='overwrite' ? '处理中…' : '覆盖本地' }}</Button>
      <Button variant="ghost" :disabled="!!resolvingAction" @click="void resolveConflict('skip')">{{ resolvingAction==='skip' ? '处理中…' : '保持现状' }}</Button>
      <p v-if="resolvingAction" class="muted" style="margin-top:8px">正在执行 {{ labelResult(resolvingAction) }}，大仓库可能耗时数秒…</p>
    </div>
    </SheetContent>
  </TooltipProvider>
</template>
