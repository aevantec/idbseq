<script setup lang="ts">
import { ref } from 'vue';
import { toast } from 'vue-sonner';

import FormField from '@/components/common/FormField.vue';
import InlineMessage from '@/components/common/InlineMessage.vue';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { describeError } from '@/lib/api';
import { COUNT_MAX, parseInteger } from '@/lib/fields';
import { formatBatch } from '@/lib/format';
import { api } from '@/session';

const props = defineProps<{ name: string }>();
const emit = defineEmits<{ taken: [] }>();

const count = ref('1');
const error = ref<string | null>(null);
const lastResult = ref<string | null>(null);
const isBusy = ref(false);

async function take(): Promise<void> {
  const parsed = parseInteger(count.value, { label: 'Count', min: 1, max: COUNT_MAX });
  if (parsed.error !== undefined) {
    error.value = parsed.error;
    return;
  }
  error.value = null;
  isBusy.value = true;
  try {
    const { data } = await api.next(props.name, parsed.value);
    lastResult.value = formatBatch(data);
    toast.success(`Took ${formatBatch(data)}.`);
    emit('taken');
  } catch (caught) {
    error.value = describeError(caught);
  } finally {
    isBusy.value = false;
  }
}
</script>

<template>
  <section class="rounded-lg border bg-card p-4" aria-labelledby="take-ids-title">
    <h2 id="take-ids-title" class="font-semibold">Take IDs</h2>
    <p class="mt-1 text-sm text-muted-foreground">Each ID taken is used up for good, exactly as if an application had taken it. Useful for testing.</p>
    <form class="mt-3 flex flex-wrap items-end gap-3" @submit.prevent="take">
      <FormField v-slot="{ id }" label="Count">
        <Input :id="id" v-model="count" inputmode="numeric" class="w-32 font-mono" />
      </FormField>
      <Button type="submit" variant="secondary" :disabled="isBusy">Take IDs</Button>
      <p v-if="lastResult" class="pb-2 font-mono text-sm text-muted-foreground" aria-live="polite">Last: {{ lastResult }}</p>
    </form>
    <InlineMessage v-if="error" class="mt-3">{{ error }}</InlineMessage>
  </section>
</template>
