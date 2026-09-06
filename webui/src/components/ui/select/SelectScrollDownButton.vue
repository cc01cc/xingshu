<script setup lang="ts">
import type { SelectScrollDownButtonProps } from "reka-ui"
import type { HTMLAttributes } from "vue"
import { reactiveOmit } from "@vueuse/core"
import { SelectScrollDownButton, useForwardProps } from "reka-ui"
import { ChevronDown } from "@lucide/vue"
import { cn } from "@/lib/utils"

const props = defineProps<SelectScrollDownButtonProps & { class?: HTMLAttributes["class"] }>()

const delegatedProps = reactiveOmit(props, "class")

const forwardedProps = useForwardProps(delegatedProps)
</script>

<template>
  <SelectScrollDownButton
    data-slot="select-scroll-down-button"
    v-bind="forwardedProps"
    :class="cn('cn-select-scroll-down-button', props.class)"
  >
    <slot>
      <ChevronDown />
    </slot>
  </SelectScrollDownButton>
</template>
