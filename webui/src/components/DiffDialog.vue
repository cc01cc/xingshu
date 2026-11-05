<script setup lang="ts">
import { ref, watch } from "vue";
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from "@/components/ui/sheet";
import { friendlyError, request } from "../lib/api";

const props = defineProps<{
  open: boolean;
  repoId: number | null;
  path: string | null;
}>();

const emit = defineEmits<{ close: [] }>();

type DiffPayload = {
  path: string;
  kind: "diff" | "binary" | "untracked" | "no_head";
  patch: string;
  truncated: boolean;
};

const loading = ref(false);
const error = ref("");
const payload = ref<DiffPayload | null>(null);

function kindLabel(kind: DiffPayload["kind"]): string {
  const map: Record<DiffPayload["kind"], string> = {
    diff: "工作区改动（相对 HEAD，只读）",
    binary: "二进制文件：不显示内容",
    untracked: "未跟踪文件：无 diff（可「打开目录」查看）",
    no_head: "仓库尚无提交（无 HEAD），无 diff",
  };
  return map[kind];
}

watch(
  () => [props.open, props.repoId, props.path] as const,
  async ([open, repoId, path]) => {
    payload.value = null;
    error.value = "";
    if (!open || repoId === null || !path) return;
    loading.value = true;
    try {
      const response = await request(`/api/v1/repos/${repoId}/diff?path=${encodeURIComponent(path)}`);
      payload.value = (await response.json()) as DiffPayload;
    } catch (reason) {
      error.value = friendlyError(reason instanceof Error ? reason.message : "加载 diff 失败");
    } finally {
      loading.value = false;
    }
  },
  { immediate: true },
);
</script>

<template>
  <Sheet :open="open" @update:open="(value: boolean) => { if (!value) emit('close'); }">
    <SheetContent v-if="open" class="diff-sheet">
      <SheetHeader class="drawer-head">
        <p class="eyebrow">READ-ONLY DIFF</p>
        <SheetTitle class="diff-title">改动预览</SheetTitle>
        <SheetDescription class="sr-only">单文件只读工作区改动（暂存与未暂存相对 HEAD）</SheetDescription>
      </SheetHeader>
      <p v-if="path" class="path" :title="path">{{ path }}</p>
      <p v-if="loading" class="muted">加载中…</p>
      <template v-else-if="payload">
        <p class="muted diff-kind">{{ kindLabel(payload.kind) }}<span v-if="payload.truncated"> · 输出过大已截断（上限 512 KiB）</span></p>
        <pre v-if="payload.kind === 'diff' && payload.patch" class="diff-patch">{{ payload.patch }}</pre>
        <p v-else-if="payload.kind === 'diff'" class="muted">该文件相对 HEAD 没有差异。</p>
      </template>
      <p v-else-if="error" class="notice error">{{ error }}</p>
    </SheetContent>
  </Sheet>
</template>
