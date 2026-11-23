<script setup lang="ts">
import { Database } from '@lucide/vue';

import PageHeader from '@/components/layout/PageHeader.vue';
</script>

<template>
  <PageHeader :crumbs="[{ label: 'Collections' }]" />
  <div class="flex flex-1 flex-col items-center justify-center gap-3 p-8 text-center">
    <Database class="size-10 text-muted-foreground" aria-hidden="true" />
    <p class="max-w-sm text-sm text-muted-foreground">
      Pick a collection on the left to see its counter, take IDs and change its settings, or create a new one.
    </p>
  </div>
</template>
