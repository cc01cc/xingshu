<script setup lang="ts">
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Ellipsis, Copy } from "@lucide/vue";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import type { Repo, Tag } from "../lib/types";
import { aheadBadgeTitle, formatBytes, statusTitle } from "../lib/format";

defineProps<{
  repos: Repo[];
  tags: Tag[];
  selected: Repo | null;
}>();

const emit = defineEmits<{
  select: [repo: Repo];
  copy: [path: string];
}>();
</script>

<template>
  <div class="table-wrap"><Table aria-label="仓库索引列表"><TableHeader><TableRow><TableHead>仓库</TableHead><TableHead>类型</TableHead><TableHead>标签</TableHead><TableHead>大小</TableHead><TableHead>状态</TableHead><TableHead /></TableRow></TableHeader><TableBody><TableRow v-for="repo in repos" :key="repo.id" :class="{selected: selected?.id===repo.id}" @click="emit('select', repo)" tabindex="0" @keydown.enter="emit('select', repo)" @keydown.space.prevent="emit('select', repo)" :aria-selected="selected?.id===repo.id">
    <TableCell><strong :title="`${repo.org}/${repo.name}`">{{ repo.org }}/{{ repo.name }}</strong><span v-if="repo.orgSource === 'path'" class="muted" style="margin-left:4px" title="未从 remote 解析出 org；按目录回退">未识别</span><small :title="repo.relPath">{{ repo.relPath }}<Button variant="ghost" size="icon-sm" class="copy-btn" @click.stop="emit('copy', repo.relPath)" title="复制路径"><Copy class="size-4" /></Button></small></TableCell><TableCell><Badge variant="outline" class="kind" :class="repo.repoKind">{{ repo.repoKind }}</Badge></TableCell><TableCell><Badge v-for="slug in repo.tags" :key="slug" variant="secondary">{{ slug }}</Badge><span v-if="!repo.tags.length" class="muted">未分类</span></TableCell><TableCell class="mono">{{ formatBytes(repo.sizeBytes) }}</TableCell><TableCell><Badge v-if="repo.cloneStatus==='broken'" variant="destructive">异常</Badge><Badge v-else-if="repo.dirty" variant="outline" class="status warning" :title="statusTitle(repo)">本地改动</Badge><Badge v-else-if="repo.ahead > 0" variant="outline" class="status warning" :title="statusTitle(repo)">领先 {{ repo.ahead }}</Badge><Badge v-else-if="repo.lastPullStatus==='ahead'" variant="outline" class="status warning" :title="aheadBadgeTitle(repo)">本地领先</Badge><Badge v-else-if="repo.lastPullStatus==='ok'" variant="outline" class="status success">正常</Badge><Badge v-else variant="outline">{{ repo.lastPullStatus ?? "未更新" }}</Badge><Badge v-if="repo.behind > 0" variant="outline" :title="statusTitle(repo)">落后 {{ repo.behind }}</Badge></TableCell><TableCell><Button variant="ghost" size="icon" class="more" @click.stop="emit('select', repo)"><Ellipsis class="size-4" /></Button></TableCell>
  </TableRow></TableBody></Table></div>
</template>
