<script setup lang="ts">
import { reactive, ref } from 'vue';

import FormField from '@/components/common/FormField.vue';
import InlineMessage from '@/components/common/InlineMessage.vue';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Textarea } from '@/components/ui/textarea';
import { parseList } from '@/lib/fields';
import type { Scope } from '@/lib/types';

const SCOPES: ReadonlyArray<{ scope: Scope; meaning: string }> = [
  { scope: 'generate', meaning: 'take IDs' },
  { scope: 'read', meaning: 'list and show collections, read the last and next ID' },
  { scope: 'manage', meaning: 'create, change, reset and delete unprotected collections' },
];

defineProps<{ isBusy?: boolean; serverErrors?: string[] }>();
const emit = defineEmits<{ submit: [body: { name: string; scopes: Scope[]; collections: string[] }] }>();

const name = ref('');
const collections = ref('');
const chosen = reactive<Record<Scope, boolean>>({ generate: false, read: false, manage: false });
const errors = ref<string[]>([]);

function submit(): void {
  const scopes = SCOPES.map(({ scope }) => scope).filter((scope) => chosen[scope]);
  errors.value = [
    ...(name.value.trim() ? [] : ['Name is required.']),
    ...(scopes.length > 0 ? [] : ['Choose at least one scope.']),
  ];
  if (errors.value.length === 0) emit('submit', { name: name.value.trim(), scopes, collections: parseList(collections.value) });
}
</script>

<template>
  <form class="grid gap-4" novalidate @submit.prevent="submit">
    <InlineMessage v-if="errors.length > 0 || (serverErrors?.length ?? 0) > 0">
      <p v-for="message in [...errors, ...(serverErrors ?? [])]" :key="message">{{ message }}</p>
    </InlineMessage>
    <FormField v-slot="{ id, describedby }" label="Name" hint="Who uses it, e.g. billing.">
      <Input :id="id" v-model="name" :aria-describedby="describedby" autocomplete="off" />
    </FormField>
    <fieldset class="grid gap-3 rounded-md border p-3">
      <legend class="px-1 text-sm font-medium">Scopes</legend>
      <div v-for="{ scope, meaning } in SCOPES" :key="scope" class="flex items-start gap-3">
        <Checkbox :id="`scope-${scope}`" v-model="chosen[scope]" class="mt-0.5" />
        <Label :for="`scope-${scope}`" class="block font-normal leading-snug">
          <code class="font-mono font-medium">{{ scope }}</code>: <span class="text-muted-foreground">{{ meaning }}</span>
        </Label>
      </div>
    </fieldset>
    <FormField v-slot="{ id, describedby }" label="Collections" hint="Names or prefixes ending in *, separated by commas or new lines. Empty: every collection.">
      <Textarea :id="id" v-model="collections" :aria-describedby="describedby" rows="2" spellcheck="false" placeholder="orders, eu.*" class="font-mono" />
    </FormField>
    <div class="flex justify-end">
      <Button type="submit" :disabled="isBusy">Create key</Button>
    </div>
  </form>
</template>
