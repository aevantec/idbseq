<script setup lang="ts">
import { useId } from 'vue';

import { Label } from '@/components/ui/label';

// The label names the control; the hint describes it (aria-describedby), so
// assistive technology reads the name without the hint.
const props = defineProps<{ label: string; hint?: string | null }>();
const id = useId();
const hintId = `${id}-hint`;
</script>

<template>
  <div class="grid content-start gap-1.5">
    <Label :for="id">{{ props.label }}</Label>
    <slot :id="id" :describedby="props.hint ? hintId : undefined" />
    <p v-if="props.hint" :id="hintId" class="text-xs text-muted-foreground">{{ props.hint }}</p>
  </div>
</template>
