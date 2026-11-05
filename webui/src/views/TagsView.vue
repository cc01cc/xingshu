<script setup lang="ts">
import { ref } from "vue";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { X } from "@lucide/vue";
import type { Repo, Tag } from "../lib/types";

const props = defineProps<{
  tags: Tag[];
  repos: Repo[];
}>();

const emit = defineEmits<{
  create: [slug: string];
  delete: [id: number];
  filter: [slug: string];
}>();

const newTag = ref("");
</script>

<template>
  <section class="tag-grid">
    <form class="tag-create" @submit.prevent="emit('create', newTag.trim())">
      <Input v-model="newTag" aria-label="新标签" placeholder="新建主题标签…" />
      <Button type="submit">添加</Button>
    </form>
    <Card v-for="tag in props.tags" :key="tag.id" data-size="sm" class="tag-card">
      <Button variant="ghost" size="icon-sm" class="tag-delete" @click.stop="emit('delete', tag.id)" aria-label="删除标签"><X class="size-4" /></Button>
      <button class="tag-card-body" @click="emit('filter', tag.slug)"><b>{{ tag.label }}</b><small>{{ props.repos.filter((repo) => repo.tags.includes(tag.slug)).length }} 个仓库</small></button>
    </Card>
  </section>
</template>
