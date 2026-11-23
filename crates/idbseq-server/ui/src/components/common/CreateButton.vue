<script setup lang="ts">
import { Plus } from '@lucide/vue';

import { Button } from '@/components/ui/button';

// Every "New …" action uses this, so they all look the same: the filled
// primary button with a plus.
</script>

<template>
  <Button type="button">
    <Plus class="size-4" aria-hidden="true" />
    <slot />
  </Button>
</template>
