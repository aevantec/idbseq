<script setup lang="ts">
import { CircleAlert, Info } from '@lucide/vue';

const props = withDefaults(defineProps<{ kind?: 'error' | 'note' }>(), { kind: 'error' });
</script>

<template>
  <div
    :role="props.kind === 'error' ? 'alert' : 'status'"
    :class="[
      'flex items-start gap-2 rounded-md border px-3 py-2 text-sm',
      props.kind === 'error' ? 'border-destructive/40 bg-destructive/10 text-destructive' : 'bg-muted text-foreground',
    ]"
  >
    <CircleAlert v-if="props.kind === 'error'" class="mt-0.5 size-4 shrink-0" aria-hidden="true" />
    <Info v-else class="mt-0.5 size-4 shrink-0" aria-hidden="true" />
    <div class="min-w-0 break-words"><slot /></div>
  </div>
</template>
