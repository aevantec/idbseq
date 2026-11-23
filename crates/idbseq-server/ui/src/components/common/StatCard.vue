<script setup lang="ts">
defineProps<{ label: string; value: string; mono?: boolean }>();
</script>

<template>
  <div class="rounded-lg border bg-card p-4">
    <dt class="text-xs font-medium text-muted-foreground">{{ label }}</dt>
    <dd :class="['mt-1 text-lg font-semibold break-all', mono && 'font-mono tabular-nums']">{{ value }}</dd>
    <slot />
  </div>
</template>
