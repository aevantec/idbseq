<script setup lang="ts">
import { watchDebounced } from '@vueuse/core';
import { Lock, Search } from '@lucide/vue';
import { onMounted, provide, ref, watch } from 'vue';
import { useRouter } from 'vue-router';
import { toast } from 'vue-sonner';

import CollectionForm from '@/components/collections/CollectionForm.vue';
import CreateButton from '@/components/common/CreateButton.vue';
import InlineMessage from '@/components/common/InlineMessage.vue';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from '@/components/ui/sheet';
import { describeError, fieldErrors } from '@/lib/api';
import { STALE, createLatest } from '@/lib/latest';
import type { Collection, CollectionSettings } from '@/lib/types';
import { api } from '@/session';
import { RELOAD_COLLECTIONS } from './collectionsContext';

const PAGE_SIZE = 100;
const SEARCH_DEBOUNCE_MS = 200;

const router = useRouter();
const prefix = ref('');
const collections = ref<Collection[]>([]);
const total = ref(0);
const cursor = ref<string | null>(null);
const error = ref<string | null>(null);
const isCreateOpen = ref(false);
const isCreating = ref(false);
const createErrors = ref<string[]>([]);

// A reopened panel starts clean, without the last attempt's errors.
watch(isCreateOpen, (isOpen) => {
  if (!isOpen) createErrors.value = [];
});
const latest = createLatest();

async function load({ append = false } = {}): Promise<void> {
  const query = { limit: PAGE_SIZE, prefix: prefix.value.trim(), cursor: append ? cursor.value : null };
  try {
    const result = await latest(() => api.listCollections(query));
    if (result === STALE) return;
    collections.value = append ? [...collections.value, ...result.data] : result.data;
    cursor.value = result.meta?.next_cursor ?? null;
    total.value = result.meta?.total ?? collections.value.length;
    error.value = null;
  } catch (caught) {
    error.value = describeError(caught);
  }
}

async function create(settings: CollectionSettings): Promise<void> {
  isCreating.value = true;
  createErrors.value = [];
  try {
    const { data } = await api.createCollection(settings);
    toast.success(`Created ${data.name}.`);
    isCreateOpen.value = false;
    await load();
    await router.push({ name: 'collection', params: { name: data.name } });
  } catch (caught) {
    createErrors.value = [describeError(caught), ...fieldErrors(caught)];
  } finally {
    isCreating.value = false;
  }
}

provide(RELOAD_COLLECTIONS, () => load());
watchDebounced(prefix, () => load(), { debounce: SEARCH_DEBOUNCE_MS });
onMounted(() => load());
</script>

<template>
  <div class="flex min-h-0 flex-1">
    <aside class="flex w-64 shrink-0 flex-col border-r bg-sidebar" aria-label="Collections list">
      <div class="flex items-center justify-between px-4 pt-4 pb-2">
        <h2 class="text-sm font-semibold">Collections</h2>
        <span class="text-xs text-muted-foreground" aria-live="polite">{{ total }} in total</span>
      </div>
      <div class="relative px-3 pb-2">
        <Search class="pointer-events-none absolute top-2.5 left-5.5 size-4 text-muted-foreground" aria-hidden="true" />
        <Input v-model="prefix" type="search" placeholder="Filter by name prefix" aria-label="Filter by name prefix" class="pl-8" />
      </div>
      <InlineMessage v-if="error" class="mx-3">{{ error }}</InlineMessage>
      <nav class="min-h-0 flex-1 overflow-y-auto px-2 pb-2" aria-label="Collections">
        <p v-if="collections.length === 0 && !error" class="px-3 py-6 text-center text-sm text-muted-foreground">No collections yet.</p>
        <RouterLink
          v-for="collection in collections"
          :key="collection.name"
          :to="{ name: 'collection', params: { name: collection.name } }"
          class="flex items-center gap-2 rounded-md px-3 py-1.5 text-sm text-muted-foreground hover:bg-accent hover:text-foreground"
          active-class="bg-accent font-medium !text-foreground"
        >
          <span class="truncate">{{ collection.name }}</span>
          <Lock v-if="collection.protected" class="ml-auto size-3.5 shrink-0" aria-label="protected" />
        </RouterLink>
        <Button v-if="cursor" variant="ghost" size="sm" class="mt-1 w-full" @click="load({ append: true })">Load more</Button>
      </nav>
      <div class="border-t p-3">
        <CreateButton class="w-full" @click="isCreateOpen = true">New collection</CreateButton>
      </div>
    </aside>

    <main class="flex min-w-0 flex-1 flex-col overflow-hidden">
      <RouterView />
    </main>

    <Sheet v-model:open="isCreateOpen">
      <SheetContent class="w-full overflow-y-auto sm:max-w-2xl">
        <SheetHeader>
          <SheetTitle>New collection</SheetTitle>
          <SheetDescription>A named counter. Its settings can be changed later.</SheetDescription>
        </SheetHeader>
        <div class="px-4 pb-4">
          <CollectionForm submit-label="Create collection" :is-busy="isCreating" :server-errors="createErrors" @submit="create" />
        </div>
      </SheetContent>
    </Sheet>
  </div>
</template>
