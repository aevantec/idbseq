<script setup lang="ts">
import { Check, Copy, TriangleAlert } from '@lucide/vue';
import { ref, useTemplateRef } from 'vue';

import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import type { CreatedKey } from '@/lib/types';

const props = defineProps<{ created: CreatedKey }>();
const emit = defineEmits<{ done: [] }>();
const copyStatus = ref<string | null>(null);
const isCopied = ref(false);
const field = useTemplateRef<{ $el: HTMLInputElement }>('field');

async function copy(): Promise<void> {
  field.value?.$el.select();
  try {
    await navigator.clipboard.writeText(props.created.key);
    isCopied.value = true;
    copyStatus.value = 'Copied.';
  } catch {
    copyStatus.value = 'The browser blocked copying: the key is selected, press Ctrl+C or ⌘C.';
  }
}
</script>

<template>
  <div class="grid gap-4">
    <div class="flex gap-3 rounded-md border border-warning/50 bg-warning/10 p-3 text-sm">
      <TriangleAlert class="mt-0.5 size-4 shrink-0 text-warning" aria-hidden="true" />
      <p>This is the only time the full key is shown. Store it like a password, for example in a secret manager.</p>
    </div>
    <div class="flex gap-2">
      <Input ref="field" :model-value="created.key" readonly aria-label="The new API key" class="font-mono text-xs" />
      <Button type="button" variant="outline" @click="copy">
        <Check v-if="isCopied" class="size-4" aria-hidden="true" /><Copy v-else class="size-4" aria-hidden="true" /> Copy
      </Button>
    </div>
    <p v-if="copyStatus" class="text-xs text-muted-foreground" aria-live="polite">{{ copyStatus }}</p>
    <div class="flex justify-end">
      <Button type="button" @click="emit('done')">I have stored it</Button>
    </div>
  </div>
</template>
