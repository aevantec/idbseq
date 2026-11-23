<script setup lang="ts">
import { computed, reactive, ref } from 'vue';

import FormField from '@/components/common/FormField.vue';
import InlineMessage from '@/components/common/InlineMessage.vue';
import NativeSelect from '@/components/common/NativeSelect.vue';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { readCollectionForm, type CollectionFormValues } from '@/lib/collectionForm';
import type { Collection, CollectionSettings } from '@/lib/types';
import { session } from '@/session';

// Collection settings, for "create" (no `initial`) and "edit".
const props = defineProps<{ initial?: Collection | null; submitLabel: string; isBusy?: boolean; serverErrors?: string[] }>();
const emit = defineEmits<{ submit: [settings: CollectionSettings] }>();

const isCreate = computed(() => !props.initial);
const values = reactive<CollectionFormValues>({
  name: '',
  start: props.initial ? String(props.initial.start) : '',
  step: props.initial ? String(props.initial.step) : '',
  max: props.initial ? String(props.initial.max) : '',
  mode: props.initial?.durability.mode ?? 'strict',
  blockSize: String(props.initial?.durability.mode === 'block' ? props.initial.durability.size : 100),
  isProtected: props.initial?.protected ?? false,
});
// Only API keys are kept out of a protected collection, and a file opened with
// `idbseqd console` has none: the flag is left as it is, and takes effect when a
// server serves the file.
const canProtect = computed(() => session.file === null);
const errors = ref<string[]>([]);
const MODES = [
  { value: 'strict', label: 'strict' },
  { value: 'block', label: 'block' },
] as const;

function submit(): void {
  const result = readCollectionForm(values, { isCreate: isCreate.value });
  errors.value = result.errors;
  if (result.errors.length === 0) emit('submit', result.value);
}
</script>

<template>
  <form class="grid gap-4" novalidate @submit.prevent="submit">
    <InlineMessage v-if="errors.length > 0 || (serverErrors?.length ?? 0) > 0">
      <p v-for="message in [...errors, ...(serverErrors ?? [])]" :key="message">{{ message }}</p>
    </InlineMessage>
    <FormField v-if="isCreate" v-slot="{ id, describedby }" label="Name" hint='Letters, digits, ".", "-" and "_".'>
      <Input :id="id" v-model="values.name" :aria-describedby="describedby" autocomplete="off" spellcheck="false" />
    </FormField>
    <div class="grid gap-4 sm:grid-cols-2">
      <FormField v-slot="{ id, describedby }" label="Start" :hint="isCreate ? 'Default 1' : null">
        <Input :id="id" v-model="values.start" :aria-describedby="describedby" inputmode="numeric" class="font-mono" :placeholder="isCreate ? '1' : ''" />
      </FormField>
      <FormField v-slot="{ id, describedby }" label="Step" :hint="isCreate ? 'Default 1' : null">
        <Input :id="id" v-model="values.step" :aria-describedby="describedby" inputmode="numeric" class="font-mono" :placeholder="isCreate ? '1' : ''" />
      </FormField>
      <FormField v-slot="{ id, describedby }" label="Max" :hint="isCreate ? 'Default: the 64-bit maximum' : null" class="sm:col-span-2">
        <Input :id="id" v-model="values.max" :aria-describedby="describedby" inputmode="numeric" class="font-mono" :placeholder="isCreate ? '9223372036854775807' : ''" />
      </FormField>
    </div>
    <div class="grid gap-4 sm:grid-cols-2">
      <FormField v-slot="{ id, describedby }" label="Durability" hint="strict: every ID is on disk before it is returned. block: faster; a crash leaves a gap, never a duplicate.">
        <NativeSelect :id="id" v-model="values.mode" :options="MODES" :aria-describedby="describedby" />
      </FormField>
      <FormField v-if="values.mode === 'block'" v-slot="{ id, describedby }" label="Block size" hint="IDs reserved per disk write.">
        <Input :id="id" v-model="values.blockSize" :aria-describedby="describedby" inputmode="numeric" class="font-mono" />
      </FormField>
    </div>
    <div v-if="canProtect" class="flex items-start gap-3 rounded-md border p-3">
      <Checkbox id="collection-protected" v-model="values.isProtected" class="mt-0.5" />
      <div class="grid gap-1">
        <Label for="collection-protected">Protected</Label>
        <p class="text-xs text-muted-foreground">Only the admin can change, reset or delete it. API keys with the manage scope cannot.</p>
      </div>
    </div>
    <div class="flex justify-end gap-2">
      <slot name="secondary" />
      <Button type="submit" :disabled="isBusy">{{ submitLabel }}</Button>
    </div>
  </form>
</template>
