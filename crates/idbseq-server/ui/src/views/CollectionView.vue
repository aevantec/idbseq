<script setup lang="ts">
import { CodeXml, Lock, Settings } from '@lucide/vue';
import { computed, inject, ref, watch } from 'vue';
import { useRouter } from 'vue-router';
import { toast } from 'vue-sonner';

import AuditEntrySheet from '@/components/audit/AuditEntrySheet.vue';
import AuditTable from '@/components/audit/AuditTable.vue';
import ApiPreviewSheet from '@/components/collections/ApiPreviewSheet.vue';
import CollectionForm from '@/components/collections/CollectionForm.vue';
import DangerZone from '@/components/collections/DangerZone.vue';
import TakeIds from '@/components/collections/TakeIds.vue';
import InlineMessage from '@/components/common/InlineMessage.vue';
import StatCard from '@/components/common/StatCard.vue';
import RefreshButton from '@/components/common/RefreshButton.vue';
import PageHeader from '@/components/layout/PageHeader.vue';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Progress } from '@/components/ui/progress';
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from '@/components/ui/sheet';
import { Skeleton } from '@/components/ui/skeleton';
import { describeError, fieldErrors } from '@/lib/api';
import { buildPatch } from '@/lib/fields';
import { formatDurability, formatId, formatRatio, formatTime, remainingRatio } from '@/lib/format';
import { STALE, createLatest } from '@/lib/latest';
import type { AuditEntry, Collection, CollectionSettings } from '@/lib/types';
import { api, session } from '@/session';
import { RELOAD_COLLECTIONS } from './collectionsContext';

const ACTIVITY_SIZE = 10;

const props = defineProps<{ name: string }>();
const router = useRouter();
const reloadList = inject(RELOAD_COLLECTIONS, async () => {});

const collection = ref<Collection | null>(null);
const activity = ref<AuditEntry[]>([]);
const error = ref<string | null>(null);
const isSettingsOpen = ref(false);
const isPreviewOpen = ref(false);
const isSaving = ref(false);
const saveErrors = ref<string[]>([]);

// A reopened panel starts clean, without the last attempt's errors.
watch(isSettingsOpen, (isOpen) => {
  if (!isOpen) saveErrors.value = [];
});
const selected = ref<AuditEntry | null>(null);
const isEntryOpen = ref(false);
const latest = createLatest();

const ratio = computed(() => (collection.value ? remainingRatio(collection.value) : 0));

/** A console on a file (`idbseqd console`) has no audit log. */
const hasAudit = computed(() => session.file === null);

async function loadActivity(): Promise<AuditEntry[]> {
  if (!hasAudit.value) return [];
  const { data } = await api.listAudit({ collection: props.name, limit: ACTIVITY_SIZE });
  return data;
}

async function load(): Promise<void> {
  try {
    const result = await latest(() => Promise.all([api.getCollection(props.name), loadActivity()]));
    if (result === STALE) return;
    collection.value = result[0].data;
    activity.value = result[1];
    error.value = null;
  } catch (caught) {
    collection.value = null;
    error.value = describeError(caught);
  }
}

async function save(settings: CollectionSettings): Promise<void> {
  const current = collection.value;
  if (!current) return;
  const { name: _name, ...editable } = settings;
  const patch = buildPatch(current, editable);
  if (Object.keys(patch).length === 0) {
    saveErrors.value = ['Nothing changed.'];
    return;
  }
  isSaving.value = true;
  saveErrors.value = [];
  try {
    await api.updateCollection(current.name, patch);
    toast.success(`Saved ${current.name}.`);
    isSettingsOpen.value = false;
    await Promise.all([load(), reloadList()]);
  } catch (caught) {
    saveErrors.value = [describeError(caught), ...fieldErrors(caught)];
  } finally {
    isSaving.value = false;
  }
}

async function afterReset(): Promise<void> {
  isSettingsOpen.value = false;
  await load();
}

async function afterDelete(): Promise<void> {
  isSettingsOpen.value = false;
  await reloadList();
  await router.push({ name: 'collections' });
}

function openEntry(entry: AuditEntry): void {
  selected.value = entry;
  isEntryOpen.value = true;
}

// Keep the current collection on screen until the next one has loaded:
// clearing it first would flash placeholders on every switch. Everything the
// page shows and acts on follows `collection`, never the route alone, so it
// stays consistent during the switch.
watch(() => props.name, () => void load(), { immediate: true });
</script>

<template>
  <PageHeader :crumbs="[{ label: 'Collections', to: '/collections' }, { label: name }]">
    <template #title-extra>
      <Badge v-if="collection?.protected" variant="outline" class="gap-1"><Lock class="size-3" aria-hidden="true" />protected</Badge>
    </template>
    <template #actions>
      <RefreshButton @click="load" />
      <Button variant="outline" :disabled="!collection" @click="isPreviewOpen = true">
        <CodeXml class="size-4" aria-hidden="true" /> API preview
      </Button>
      <Button variant="outline" :disabled="!collection" @click="isSettingsOpen = true">
        <Settings class="size-4" aria-hidden="true" /> Settings
      </Button>
    </template>
  </PageHeader>

  <div class="flex-1 overflow-y-auto">
    <div class="mx-auto grid max-w-5xl gap-6 p-6" :aria-busy="collection !== null && collection.name !== name">
      <InlineMessage v-if="error">{{ error }}</InlineMessage>
      <div v-else-if="!collection" class="grid gap-4 sm:grid-cols-4">
        <Skeleton v-for="index in 4" :key="index" class="h-20" />
      </div>
      <template v-else>
        <dl class="summary grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
          <StatCard label="Last ID" :value="formatId(collection.last_id)" mono />
          <StatCard label="Next ID" :value="formatId(collection.next_id)" mono />
          <StatCard label="Range left" :value="formatRatio(ratio)" mono>
            <Progress :model-value="ratio * 100" class="mt-2 h-1.5" aria-label="Range left" />
          </StatCard>
          <StatCard label="Durability" :value="formatDurability(collection.durability)" />
        </dl>
        <p class="-mt-2 text-xs text-muted-foreground">
          Start <span class="font-mono">{{ collection.start }}</span> · step <span class="font-mono">{{ collection.step }}</span> ·
          max <span class="font-mono">{{ collection.max }}</span> · created {{ formatTime(collection.created_at) }}
        </p>

        <TakeIds :name="collection.name" @taken="load" />

        <section v-if="hasAudit" aria-labelledby="activity-title" class="rounded-lg border bg-card">
          <div class="flex items-center justify-between px-4 pt-4">
            <h2 id="activity-title" class="font-semibold">Recent changes</h2>
            <RouterLink :to="{ name: 'audit', query: { collection: collection.name } }" class="text-sm text-muted-foreground hover:text-foreground">Open in audit log →</RouterLink>
          </div>
          <div class="px-2 pb-2">
            <AuditTable :entries="activity" empty-text="No changes recorded yet." @select="openEntry" />
          </div>
        </section>
      </template>
    </div>
  </div>

  <Sheet v-model:open="isSettingsOpen">
    <SheetContent class="w-full overflow-y-auto sm:max-w-2xl">
      <SheetHeader>
        <SheetTitle>{{ collection?.name ?? name }} settings</SheetTitle>
        <SheetDescription>Only the fields you change are sent.</SheetDescription>
      </SheetHeader>
      <div v-if="collection" class="grid gap-6 px-4 pb-6">
        <CollectionForm :key="`${collection.name}:${collection.created_at}:${collection.step}`" :initial="collection" submit-label="Save changes" :is-busy="isSaving" :server-errors="saveErrors" @submit="save" />
        <DangerZone :collection="collection" @reset="afterReset" @deleted="afterDelete" />
      </div>
    </SheetContent>
  </Sheet>

  <AuditEntrySheet v-model:open="isEntryOpen" :entry="selected" />
  <ApiPreviewSheet v-if="collection" v-model:open="isPreviewOpen" :collection="collection" />
</template>
