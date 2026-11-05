<script setup lang="ts">
import { Button } from "@/components/ui/button";

defineProps<{
  view: "repos" | "tags" | "disks" | "settings" | "policy";
  reposCount: number;
  tagsCount: number;
  conflictsCount: number;
  mobileNavOpen: boolean;
  isBusy: boolean;
  scanning: boolean;
  pulling: boolean;
}>();

const emit = defineEmits<{
  setView: [view: "repos" | "tags" | "disks" | "settings" | "policy"];
  scan: [];
  pull: [];
  closeMobileNav: [];
}>();
</script>

<template>
  <aside class="sidebar" :class="{ open: mobileNavOpen }">
    <div class="brand"><span class="brand-mark">✦</span><span><strong>星枢</strong><small>XINGSHU</small></span></div>
    <nav>
      <a :class="{ active: view === 'repos' }" href="#" @click.prevent="emit('setView', 'repos')">仓库 <span>{{ reposCount }}</span></a>
      <a :class="{ active: view === 'tags' }" href="#" @click.prevent="emit('setView', 'tags')">标签 <span :class="{ 'count-zero': tagsCount === 0 }">{{ tagsCount }}</span></a>
      <a :class="{ active: view === 'disks' }" href="#" @click.prevent="emit('setView', 'disks')">磁盘看板</a>
      <a :class="{ active: view === 'settings' }" href="#" @click.prevent="emit('setView', 'settings')">设置</a>
      <a :class="{ active: view === 'policy' }" href="#" @click.prevent="emit('setView', 'policy')">策略</a>
      <a v-if="conflictsCount" class="attention" href="#" @click.prevent="emit('setView', 'repos')">待决策 <span>{{ conflictsCount }}</span></a>
      <Button variant="outline" size="sm" class="mt-3 w-full" :disabled="isBusy" @click="emit('scan')">{{ scanning ? '扫描中…' : '扫描索引' }}</Button>
      <Button variant="outline" size="sm" class="mt-2 w-full" :disabled="isBusy" @click="emit('pull')">{{ pulling ? '批量 pull 中…' : '批量 pull' }}</Button>
    </nav>
    <div class="sidebar-note">本地仓库索引<br /><small>localhost only</small></div>
  </aside>
  <div v-if="mobileNavOpen" class="sidebar-backdrop" @click="emit('closeMobileNav')"></div>
</template>
