<script setup lang="ts">
import { RefreshCw } from '@lucide/vue';

import { Button } from '@/components/ui/button';

// The one refresh control in every page header: an icon, named for screen readers and on hover.
defineEmits<{ click: [] }>();
</script>

<template>
  <Button variant="ghost" size="icon" aria-label="Refresh" title="Refresh" @click="$emit('click')">
    <RefreshCw class="size-4" aria-hidden="true" />
  </Button>
</template>
