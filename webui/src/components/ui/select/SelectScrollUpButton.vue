<script setup lang="ts">
import type { SelectScrollUpButtonProps } from "reka-ui"
import type { HTMLAttributes } from "vue"
import { reactiveOmit } from "@vueuse/core"
import { SelectScrollUpButton, useForwardProps } from "reka-ui"
import { ChevronUp } from "@lucide/vue"
import { cn } from "@/lib/utils"

const props = defineProps<SelectScrollUpButtonProps & { class?: HTMLAttributes["class"] }>()

const delegatedProps = reactiveOmit(props, "class")

const forwardedProps = useForwardProps(delegatedProps)
</script>

<template>
  <SelectScrollUpButton
    data-slot="select-scroll-up-button"
    v-bind="forwardedProps"
    :class="cn('cn-select-scroll-up-button', props.class)"
  >
    <slot>
      <ChevronUp />
    </slot>
  </SelectScrollUpButton>
</template>
