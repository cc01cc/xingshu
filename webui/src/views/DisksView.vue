<script setup lang="ts">
import { Badge } from "@/components/ui/badge";
import { Card } from "@/components/ui/card";
import { formatBytes } from "../lib/format";
import type { Repo, Root, Stats } from "../lib/types";

defineProps<{
  stats: Stats;
  conflicts: number;
  roots: Root[];
  repos: Repo[];
}>();
</script>

<template>
  <section class="dashboard">
    <Card class="stat-card"><small>索引仓库</small><strong>{{ stats.repositories }}</strong></Card>
    <Card class="stat-card"><small>已占用空间</small><strong>{{ formatBytes(stats.bytes) }}</strong></Card>
    <Card class="stat-card"><small>待决策</small><strong class="accent">{{ conflicts }}</strong></Card>
  </section>
  <div v-if="Object.keys(stats.byKind).length" class="kind-bar">
    <div v-for="(count, kind) in stats.byKind" :key="kind" class="kind-segment" :style="{ flex: count }"><Badge variant="outline" class="kind" :class="kind">{{ kind }}</Badge> {{ count }}</div>
  </div>
  <div v-if="roots.length" style="margin-top:16px;display:grid;gap:8px;">
    <div v-for="root in roots" :key="root.id" class="root-item">
      <div><strong>{{ root.name }}</strong><small>{{ root.path }}</small></div>
      <span class="mono">{{ repos.filter(r => r.rootId === root.id).length }} 仓</span>
    </div>
  </div>
</template>
