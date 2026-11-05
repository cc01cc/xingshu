<script setup lang="ts">
import { computed, ref } from "vue";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { friendlyError, request } from "../lib/api";
import type { Repo, Tag } from "../lib/types";

type PolicyForm = { pullStrategy: string; conflict: string; unattended: string };
const defaultForm = (): PolicyForm => ({ pullStrategy: "fetch-only", conflict: "stop", unattended: "skip" });

const props = defineProps<{ repos: Repo[]; tags: Tag[] }>();
const emit = defineEmits<{ notice: [message: string] }>();

const repoId = ref<number | null>(null);
const repoForm = ref<PolicyForm>(defaultForm());
const source = ref("kind-default");
const sourceTagId = ref<number | null>(null);
const tagId = ref<number | null>(null);
const tagForm = ref<PolicyForm>(defaultForm());
const busy = ref(false);

const sourceLabel = computed(() => {
  if (source.value === "repo") return "来源：仓库级策略";
  if (source.value === "tag") {
    const tag = props.tags.find((item) => item.id === sourceTagId.value);
    return `来源：标签「${tag?.label ?? sourceTagId.value}」（多标签按顺序取第一个）`;
  }
  return "来源：类型默认";
});
const tagMembers = computed(() => {
  const tag = props.tags.find((item) => item.id === tagId.value);
  return tag ? props.repos.filter((repo) => repo.tags.includes(tag.slug)).length : 0;
});

async function loadRepoPolicy(): Promise<void> {
  if (repoId.value === null) return;
  busy.value = true;
  try {
    const response = await request(`/api/v1/repos/${repoId.value}/policy`);
    const data = (await response.json()) as Record<string, unknown>;
    const effective = (data.effective ?? {}) as Record<string, unknown>;
    repoForm.value = {
      pullStrategy: String(effective.pullStrategy ?? data.pullStrategy ?? "fetch-only"),
      conflict: String(effective.conflict ?? data.pullConflictPolicy ?? "stop"),
      unattended: String(effective.unattended ?? data.unattendedConflictPolicy ?? "skip"),
    };
    source.value = String(data.source ?? "kind-default");
    sourceTagId.value =
      data.sourceTagId === null || data.sourceTagId === undefined ? null : Number(data.sourceTagId);
  } catch (reason) {
    emit("notice", friendlyError(reason instanceof Error ? reason.message : "加载策略失败"));
  } finally {
    busy.value = false;
  }
}

async function saveRepoPolicy(): Promise<void> {
  if (repoId.value === null) return;
  busy.value = true;
  try {
    await request(`/api/v1/repos/${repoId.value}/policy`, { method: "PUT", headers: { "Content-Type": "application/json" }, body: JSON.stringify(repoForm.value) });
    emit("notice", "仓库策略已保存");
    await loadRepoPolicy();
  } catch (reason) {
    emit("notice", friendlyError(reason instanceof Error ? reason.message : "保存策略失败"));
  } finally {
    busy.value = false;
  }
}

async function loadTagPolicy(): Promise<void> {
  if (tagId.value === null) return;
  busy.value = true;
  try {
    const response = await request(`/api/v1/tags/${tagId.value}/policy`);
    const data = (await response.json()) as Record<string, unknown> | null;
    tagForm.value = data
      ? {
          pullStrategy: String(data.pullStrategy ?? "fetch-only"),
          conflict: String(data.pullConflictPolicy ?? "stop"),
          unattended: String(data.unattendedConflictPolicy ?? "skip"),
        }
      : defaultForm();
  } catch (reason) {
    emit("notice", friendlyError(reason instanceof Error ? reason.message : "加载标签策略失败"));
  } finally {
    busy.value = false;
  }
}

async function saveTagPolicy(): Promise<void> {
  if (tagId.value === null) return;
  busy.value = true;
  try {
    await request(`/api/v1/tags/${tagId.value}/policy`, { method: "PUT", headers: { "Content-Type": "application/json" }, body: JSON.stringify(tagForm.value) });
    emit("notice", "标签策略已保存（未单独设置仓库策略的成员仓将继承）");
    await loadTagPolicy();
  } catch (reason) {
    emit("notice", friendlyError(reason instanceof Error ? reason.message : "保存标签策略失败"));
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <h2 class="settings-title">策略管理</h2>
  <p class="settings-hint">按仓库或主题标签配置拉取策略与冲突处理；生效顺序为 仓库级 &gt; 标签（按顺序取第一个）&gt; 类型默认。</p>
  <div class="policy-block">
    <div class="root-create">
      <Select :model-value="repoId === null ? '' : String(repoId)" @update:model-value="repoId = $event === '' ? null : Number($event); source = 'kind-default'; sourceTagId = null">
        <SelectTrigger class="w-64" aria-label="选择仓库"><SelectValue placeholder="选择仓库…" /></SelectTrigger>
        <SelectContent><SelectItem v-for="repo in props.repos" :key="repo.id" :value="String(repo.id)">{{ repo.org }}/{{ repo.name }}</SelectItem></SelectContent>
      </Select>
      <Button variant="outline" size="sm" :disabled="busy" @click="void loadRepoPolicy()">加载</Button>
      <Badge v-if="repoId !== null" variant="secondary">{{ sourceLabel }}</Badge>
    </div>
    <div v-if="repoId !== null" class="policy-form" style="display:grid;gap:8px;max-width:480px;">
      <div class="policy-field"><Label>拉取策略</Label><Select v-model="repoForm.pullStrategy"><SelectTrigger class="w-44"><SelectValue /></SelectTrigger><SelectContent><SelectItem value="fetch-only">fetch-only</SelectItem><SelectItem value="mirror">mirror</SelectItem><SelectItem value="no-update">no-update</SelectItem><SelectItem value="archive">archive</SelectItem></SelectContent></Select></div>
      <div class="policy-field"><Label>冲突策略</Label><Select v-model="repoForm.conflict"><SelectTrigger class="w-44"><SelectValue /></SelectTrigger><SelectContent><SelectItem value="stop">stop</SelectItem><SelectItem value="backup">backup</SelectItem><SelectItem value="overwrite">overwrite</SelectItem><SelectItem value="skip">skip</SelectItem></SelectContent></Select></div>
      <div class="policy-field"><Label>无人值守</Label><Select v-model="repoForm.unattended"><SelectTrigger class="w-44"><SelectValue /></SelectTrigger><SelectContent><SelectItem value="stop">stop</SelectItem><SelectItem value="backup">backup</SelectItem><SelectItem value="overwrite">overwrite</SelectItem><SelectItem value="skip">skip</SelectItem></SelectContent></Select></div>
      <Button :disabled="busy" @click="void saveRepoPolicy()">保存仓库策略</Button>
    </div>
  </div>
  <h3 class="settings-title policy-subtitle">标签策略</h3>
  <p class="settings-hint">标签策略对未单独设置仓库策略的成员仓生效（继承默认层），不会覆盖已有的仓库级例外。</p>
  <div class="policy-block">
    <div class="root-create">
      <Select :model-value="tagId === null ? '' : String(tagId)" @update:model-value="tagId = $event === '' ? null : Number($event)">
        <SelectTrigger class="w-64" aria-label="选择标签"><SelectValue placeholder="选择标签…" /></SelectTrigger>
        <SelectContent><SelectItem v-for="tag in props.tags" :key="tag.id" :value="String(tag.id)">{{ tag.label }}</SelectItem></SelectContent>
      </Select>
      <Button variant="outline" size="sm" :disabled="busy" @click="void loadTagPolicy()">加载</Button>
      <Badge v-if="tagId !== null" variant="secondary">{{ tagMembers }} 个成员仓库</Badge>
    </div>
    <div v-if="tagId !== null" class="policy-form" style="display:grid;gap:8px;max-width:480px;">
      <div class="policy-field"><Label>拉取策略</Label><Select v-model="tagForm.pullStrategy"><SelectTrigger class="w-44"><SelectValue /></SelectTrigger><SelectContent><SelectItem value="fetch-only">fetch-only</SelectItem><SelectItem value="mirror">mirror</SelectItem><SelectItem value="no-update">no-update</SelectItem><SelectItem value="archive">archive</SelectItem></SelectContent></Select></div>
      <div class="policy-field"><Label>冲突策略</Label><Select v-model="tagForm.conflict"><SelectTrigger class="w-44"><SelectValue /></SelectTrigger><SelectContent><SelectItem value="stop">stop</SelectItem><SelectItem value="backup">backup</SelectItem><SelectItem value="overwrite">overwrite</SelectItem><SelectItem value="skip">skip</SelectItem></SelectContent></Select></div>
      <div class="policy-field"><Label>无人值守</Label><Select v-model="tagForm.unattended"><SelectTrigger class="w-44"><SelectValue /></SelectTrigger><SelectContent><SelectItem value="stop">stop</SelectItem><SelectItem value="backup">backup</SelectItem><SelectItem value="overwrite">overwrite</SelectItem><SelectItem value="skip">skip</SelectItem></SelectContent></Select></div>
      <Button :disabled="busy" @click="void saveTagPolicy()">保存标签策略</Button>
    </div>
  </div>
</template>
