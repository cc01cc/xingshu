<script setup lang="ts">
import { ref } from "vue";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import type { Repo, Root } from "../lib/types";

defineProps<{
  roots: Root[];
  repos: Repo[];
  isBusy: boolean;
  scanning: boolean;
  hasTask: boolean;
}>();

const emit = defineEmits<{
  addRoot: [path: string];
  removeRoot: [id: number];
  scan: [];
}>();

const newRootPath = ref("");
</script>

<template>
  <h2 class="settings-title">根目录管理</h2>
  <p class="settings-hint">添加本地 Git 仓库的父目录（可多个、跨盘），添加后需手动触发扫描才会生成索引。</p>
  <form class="root-create" @submit.prevent="emit('addRoot', newRootPath.trim())">
    <Input v-model="newRootPath" aria-label="根目录路径" placeholder="输入根目录绝对路径…" />
    <Button type="submit">添加</Button>
  </form>
  <div class="root-list">
    <div v-for="root in roots" :key="root.id" class="root-item"><div><strong>{{ root.name }}</strong><small>{{ root.path }}</small><Badge v-if="root.diskLabel" variant="secondary">{{ root.diskLabel }}</Badge></div><Button variant="ghost" size="sm" @click="emit('removeRoot', root.id)">移除</Button></div>
    <div v-if="!roots.length" class="empty">暂无根目录 · 添加包含 Git 仓库的父目录以开始</div>
  </div>
  <div class="settings-actions">
    <Button :disabled="isBusy || !roots.length" @click="emit('scan')">{{ scanning ? '扫描中…' : `扫描索引（${roots.length} 个根目录）` }}</Button>
    <span class="muted">{{ roots.length ? (repos.length ? `已索引 ${repos.length} 个仓库` : '尚未扫描，点击扫描后仓库将出现在「仓库」页') : '先添加根目录' }}</span>
  </div>
  <p v-if="roots.length && !repos.length && !hasTask" class="notice">提示：根目录已就绪，请点击上方「扫描索引」执行首次索引。空目录不会产生仓库。</p>
  <h2 class="settings-title">关于</h2>
  <dl class="about"><dt>版本</dt><dd>0.1.0</dd><dt>API 端口</dt><dd>12681</dd><dt>WebUI 端口</dt><dd>12680</dd></dl>
</template>
