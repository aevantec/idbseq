<script setup lang="ts">
import { onMounted, ref } from 'vue';
import { useRoute } from 'vue-router';

import AuditEntrySheet from '@/components/audit/AuditEntrySheet.vue';
import AuditTable from '@/components/audit/AuditTable.vue';
import FormField from '@/components/common/FormField.vue';
import InlineMessage from '@/components/common/InlineMessage.vue';
import NativeSelect from '@/components/common/NativeSelect.vue';
import RefreshButton from '@/components/common/RefreshButton.vue';
import PageHeader from '@/components/layout/PageHeader.vue';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { describeError } from '@/lib/api';
import { STALE, createLatest } from '@/lib/latest';
import { AUDIT_ACTIONS, type AuditAction, type AuditEntry } from '@/lib/types';
import { api } from '@/session';

const PAGE_SIZE = 50;
const ACTION_OPTIONS = [{ value: '' as const, label: 'any' }, ...AUDIT_ACTIONS.map((action) => ({ value: action, label: action }))];

const route = useRoute();
const collection = ref(typeof route.query.collection === 'string' ? route.query.collection : '');
const action = ref<AuditAction | ''>('');
const entries = ref<AuditEntry[]>([]);
const cursor = ref<string | null>(null);
const error = ref<string | null>(null);
const selected = ref<AuditEntry | null>(null);
const isEntryOpen = ref(false);
const latest = createLatest();
// The filters of the list on screen: "Load more" must continue the same query
// even if the inputs were edited since.
let applied = { collection: '', action: '' as AuditAction | '' };

async function load({ append = false } = {}): Promise<void> {
  if (!append) applied = { collection: collection.value.trim(), action: action.value };
  const query = { limit: PAGE_SIZE, ...applied, cursor: append ? cursor.value : null };
  try {
    const result = await latest(() => api.listAudit(query));
    if (result === STALE) return;
    entries.value = append ? [...entries.value, ...result.data] : result.data;
    cursor.value = result.meta?.next_cursor ?? null;
    error.value = null;
  } catch (caught) {
    error.value = describeError(caught);
  }
}

function openEntry(entry: AuditEntry): void {
  selected.value = entry;
  isEntryOpen.value = true;
}

onMounted(() => load());
</script>

<template>
  <PageHeader :crumbs="[{ label: 'Audit log' }]">
    <template #actions>
      <RefreshButton @click="load()" />
    </template>
  </PageHeader>

  <div class="flex-1 overflow-y-auto">
    <div class="mx-auto grid max-w-6xl gap-4 p-6">
      <p class="text-sm text-muted-foreground">Every change to collections, keys and the admin account, newest first. Taking IDs is not audited.</p>
      <form class="flex flex-wrap items-end gap-3" @submit.prevent="load()">
        <FormField v-slot="{ id }" label="Collection">
          <Input :id="id" v-model="collection" type="search" placeholder="any" class="w-56" />
        </FormField>
        <FormField v-slot="{ id }" label="Action">
          <NativeSelect :id="id" v-model="action" :options="ACTION_OPTIONS" class="w-56" @change="load()" />
        </FormField>
        <Button type="submit" variant="secondary">Filter</Button>
      </form>
      <InlineMessage v-if="error">{{ error }}</InlineMessage>
      <div class="rounded-lg border bg-card">
        <AuditTable :entries="entries" show-collection empty-text="No entries match." @select="openEntry" />
      </div>
      <Button v-if="cursor" variant="outline" class="justify-self-center" @click="load({ append: true })">Load more</Button>
    </div>
  </div>

  <AuditEntrySheet v-model:open="isEntryOpen" :entry="selected" />
</template>
