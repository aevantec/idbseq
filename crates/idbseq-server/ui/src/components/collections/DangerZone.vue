<script setup lang="ts">
import { ref } from 'vue';
import { toast } from 'vue-sonner';

import ConfirmDialog from '@/components/common/ConfirmDialog.vue';
import FormField from '@/components/common/FormField.vue';
import InlineMessage from '@/components/common/InlineMessage.vue';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { describeError, fieldErrors } from '@/lib/api';
import { parseInteger } from '@/lib/fields';
import { formatId } from '@/lib/format';
import type { Collection, Id } from '@/lib/types';
import { api } from '@/session';

const REWIND_WARNING = 'Rewinding hands out IDs that were already issued, so they can be duplicated. Only do it when none of them is in use.';

const props = defineProps<{ collection: Collection }>();
const emit = defineEmits<{ reset: []; deleted: [] }>();

const resetValue = ref('');
const allowRewind = ref(false);
const error = ref<string | null>(null);
const isResetOpen = ref(false);
const isDeleteOpen = ref(false);
const pendingValue = ref<Id | null>(null);

function askReset(): void {
  const text = resetValue.value.trim();
  if (text === '') {
    pendingValue.value = null;
  } else {
    const parsed = parseInteger(text, { label: 'Next ID' });
    if (parsed.error !== undefined) {
      error.value = parsed.error;
      return;
    }
    pendingValue.value = parsed.value;
  }
  error.value = null;
  isResetOpen.value = true;
}

async function reset(): Promise<void> {
  const { name } = props.collection;
  try {
    const { data } = await api.reset(name, { value: pendingValue.value, allow_rewind: allowRewind.value, confirm: name });
    toast.success(`Reset ${name}: the next ID is ${formatId(data.next_id)}.`);
    resetValue.value = '';
    emit('reset');
  } catch (caught) {
    error.value = [describeError(caught), ...fieldErrors(caught)].join(' ');
  }
}

async function remove(): Promise<void> {
  const { name } = props.collection;
  try {
    await api.deleteCollection(name, name);
    toast.success(`Deleted ${name}.`);
    emit('deleted');
  } catch (caught) {
    error.value = describeError(caught);
  }
}
</script>

<template>
  <section class="grid gap-4 rounded-lg border border-destructive/40 p-4" aria-labelledby="danger-title">
    <h3 id="danger-title" class="font-semibold text-destructive">Danger zone</h3>
    <InlineMessage v-if="error">{{ error }}</InlineMessage>

    <form class="grid gap-3" @submit.prevent="askReset">
      <div>
        <h4 class="text-sm font-medium">Reset the counter</h4>
        <p class="text-xs text-muted-foreground">Set the next ID to a new value. Leave it empty to go back to the start ({{ collection.start }}).</p>
      </div>
      <FormField v-slot="{ id }" label="Next ID">
        <Input :id="id" v-model="resetValue" inputmode="numeric" class="font-mono" :placeholder="String(collection.start)" />
      </FormField>
      <div class="flex items-start gap-3">
        <Checkbox id="allow-rewind" v-model="allowRewind" class="mt-0.5" />
        <div class="grid gap-1">
          <Label for="allow-rewind">Allow rewind</Label>
          <p class="text-xs text-destructive">{{ REWIND_WARNING }}</p>
        </div>
      </div>
      <div class="flex justify-end">
        <Button type="submit" variant="outline" class="border-destructive/50 text-destructive hover:bg-destructive/10">Reset counter</Button>
      </div>
    </form>

    <div class="flex flex-wrap items-center justify-between gap-3 border-t pt-4">
      <div>
        <h4 class="text-sm font-medium">Delete the collection</h4>
        <p class="text-xs text-muted-foreground">Removes it and its counter.</p>
      </div>
      <Button type="button" variant="destructive" @click="isDeleteOpen = true">Delete collection</Button>
    </div>

    <ConfirmDialog
      v-model:open="isResetOpen"
      :title="`Reset ${collection.name}?`"
      :paragraphs="[
        `The next ID will be ${pendingValue === null ? `its start, ${collection.start}` : pendingValue}.`,
        allowRewind ? REWIND_WARNING : 'Moving backwards is refused unless “Allow rewind” is ticked.',
      ]"
      confirm-label="Reset collection"
      :type-to-confirm="collection.name"
      @confirm="reset"
    />
    <ConfirmDialog
      v-model:open="isDeleteOpen"
      :title="`Delete ${collection.name}?`"
      :paragraphs="['The collection and its counter are removed for good. Creating it again starts from scratch, so earlier IDs can be issued again.']"
      confirm-label="Delete collection"
      :type-to-confirm="collection.name"
      @confirm="remove"
    />
  </section>
</template>
