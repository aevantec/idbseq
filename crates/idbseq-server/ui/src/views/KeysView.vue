<script setup lang="ts">
import { onMounted, ref } from 'vue';
import { toast } from 'vue-sonner';

import ConfirmDialog from '@/components/common/ConfirmDialog.vue';
import CreateButton from '@/components/common/CreateButton.vue';
import InlineMessage from '@/components/common/InlineMessage.vue';
import KeyForm from '@/components/keys/KeyForm.vue';
import SecretPanel from '@/components/keys/SecretPanel.vue';
import RefreshButton from '@/components/common/RefreshButton.vue';
import PageHeader from '@/components/layout/PageHeader.vue';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from '@/components/ui/sheet';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import { describeError, fieldErrors } from '@/lib/api';
import { formatTime } from '@/lib/format';
import { STALE, createLatest } from '@/lib/latest';
import type { ApiKey, CreatedKey, Scope } from '@/lib/types';
import { api } from '@/session';

const keys = ref<ApiKey[]>([]);
const error = ref<string | null>(null);
const isSheetOpen = ref(false);
const isCreating = ref(false);
const createErrors = ref<string[]>([]);
const created = ref<CreatedKey | null>(null);
const revoking = ref<ApiKey | null>(null);
const isRevokeOpen = ref(false);
const latest = createLatest();

async function load(): Promise<void> {
  try {
    const result = await latest(() => api.listKeys());
    if (result === STALE) return;
    keys.value = result.data;
    error.value = null;
  } catch (caught) {
    error.value = describeError(caught);
  }
}

function openCreate(): void {
  created.value = null;
  createErrors.value = [];
  isSheetOpen.value = true;
}

async function create(body: { name: string; scopes: Scope[]; collections: string[] }): Promise<void> {
  isCreating.value = true;
  createErrors.value = [];
  try {
    const { data } = await api.createKey(body);
    created.value = data;
    toast.success(`Created ${data.name} (${data.id}). Copy the key now.`);
    await load();
  } catch (caught) {
    createErrors.value = [describeError(caught), ...fieldErrors(caught)];
  } finally {
    isCreating.value = false;
  }
}

function closeSheet(): void {
  isSheetOpen.value = false;
  created.value = null;
}

function askRevoke(key: ApiKey): void {
  revoking.value = key;
  isRevokeOpen.value = true;
}

async function revoke(): Promise<void> {
  const key = revoking.value;
  if (!key) return;
  try {
    await api.revokeKey(key.id);
    toast.success(`Revoked ${key.name} (${key.id}).`);
    await load();
  } catch (caught) {
    toast.error(describeError(caught));
  }
}

onMounted(load);
</script>

<template>
  <PageHeader :crumbs="[{ label: 'API keys' }]">
    <template #actions>
      <RefreshButton @click="load" />
      <CreateButton @click="openCreate">New API key</CreateButton>
    </template>
  </PageHeader>

  <div class="flex-1 overflow-y-auto">
    <div class="mx-auto grid max-w-6xl gap-4 p-6">
      <p class="text-sm text-muted-foreground">
        Give every application its own key with the fewest scopes it needs. The server keeps only a hash: a key is shown once, when it is created.
      </p>
      <InlineMessage v-if="error">{{ error }}</InlineMessage>
      <div class="rounded-lg border bg-card">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Name</TableHead>
              <TableHead>ID</TableHead>
              <TableHead>Scopes</TableHead>
              <TableHead>Collections</TableHead>
              <TableHead>Created</TableHead>
              <TableHead>Last used</TableHead>
              <TableHead><span class="sr-only">Actions</span></TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            <TableRow v-if="keys.length === 0">
              <TableCell colspan="7" class="py-8 text-center text-muted-foreground">No keys yet.</TableCell>
            </TableRow>
            <TableRow v-for="key in keys" :key="key.id">
              <TableCell class="font-medium">{{ key.name }}</TableCell>
              <TableCell class="font-mono text-xs">{{ key.id }}</TableCell>
              <TableCell><Badge v-for="scope in key.scopes" :key="scope" variant="secondary" class="mr-1 font-mono">{{ scope }}</Badge></TableCell>
              <TableCell class="font-mono text-xs">
                <template v-if="key.collections.length > 0">{{ key.collections.join(', ') }}</template>
                <span v-else class="font-sans text-muted-foreground">all</span>
              </TableCell>
              <TableCell class="whitespace-nowrap">{{ formatTime(key.created_at) }}</TableCell>
              <TableCell class="whitespace-nowrap">
                <template v-if="key.last_used_at">{{ formatTime(key.last_used_at) }}</template>
                <span v-else class="text-muted-foreground">never</span>
              </TableCell>
              <TableCell class="text-right">
                <Button variant="ghost" size="sm" class="text-destructive hover:text-destructive" @click="askRevoke(key)">Revoke</Button>
              </TableCell>
            </TableRow>
          </TableBody>
        </Table>
      </div>
    </div>
  </div>

  <Sheet :open="isSheetOpen" @update:open="(isOpen) => (isOpen ? (isSheetOpen = true) : closeSheet())">
    <SheetContent class="w-full overflow-y-auto sm:max-w-2xl">
      <SheetHeader>
        <SheetTitle>{{ created ? `Key for ${created.name}` : 'New API key' }}</SheetTitle>
        <SheetDescription>{{ created ? `Key ${created.id} is ready.` : 'Scopes and collections limit what the key can do.' }}</SheetDescription>
      </SheetHeader>
      <div class="px-4 pb-6">
        <SecretPanel v-if="created" :created="created" @done="closeSheet" />
        <KeyForm v-else :is-busy="isCreating" :server-errors="createErrors" @submit="create" />
      </div>
    </SheetContent>
  </Sheet>

  <ConfirmDialog
    v-model:open="isRevokeOpen"
    :title="`Revoke ${revoking?.name ?? ''}?`"
    :paragraphs="[`Key ${revoking?.id ?? ''} stops working immediately. Anything still using it gets 401 UNAUTHORIZED.`]"
    confirm-label="Revoke key"
    @confirm="revoke"
  />
</template>
