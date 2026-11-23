<script setup lang="ts">
import { computed } from 'vue';

import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from '@/components/ui/sheet';
import { formatTime, summarizeChange } from '@/lib/format';
import type { AuditEntry } from '@/lib/types';

const open = defineModel<boolean>('open', { required: true });
const props = defineProps<{ entry: AuditEntry | null }>();

const details = computed(() => {
  const entry = props.entry;
  if (!entry) return [];
  return [
    ['When', formatTime(entry.at)],
    ['Who', entry.actor],
    ['Collection', entry.collection ?? '—'],
    ['Client IP', entry.client_ip ?? '—'],
    ['Request ID', entry.request_id ?? '—'],
  ];
});
const changes = computed(() => (props.entry ? summarizeChange(props.entry.previous, props.entry.new) : []));
</script>

<template>
  <Sheet v-model:open="open">
    <SheetContent class="w-full overflow-y-auto sm:max-w-2xl">
      <SheetHeader>
        <SheetTitle class="font-mono">{{ entry?.action }}</SheetTitle>
        <SheetDescription>Audit entry #{{ entry?.id }}</SheetDescription>
      </SheetHeader>
      <div class="grid gap-6 px-4 pb-6">
        <dl class="grid grid-cols-[auto_1fr] gap-x-6 gap-y-2 text-sm">
          <template v-for="[term, value] in details" :key="term">
            <dt class="text-muted-foreground">{{ term }}</dt>
            <dd class="font-mono break-all">{{ value }}</dd>
          </template>
        </dl>
        <section v-if="changes.length > 0" aria-labelledby="changes-title">
          <h3 id="changes-title" class="mb-2 text-sm font-semibold">Changes</h3>
          <div class="overflow-hidden rounded-md border">
            <table class="w-full text-sm">
              <thead class="bg-muted/50 text-left text-xs text-muted-foreground">
                <tr><th class="px-3 py-2 font-medium">Field</th><th class="px-3 py-2 font-medium">Before</th><th class="px-3 py-2 font-medium">After</th></tr>
              </thead>
              <tbody>
                <tr v-for="row in changes" :key="row.field" class="border-t align-top">
                  <th scope="row" class="px-3 py-2 text-left font-mono font-medium">{{ row.field }}</th>
                  <td class="px-3 py-2 font-mono break-all text-destructive">{{ row.before }}</td>
                  <td class="px-3 py-2 font-mono break-all text-success">{{ row.after }}</td>
                </tr>
              </tbody>
            </table>
          </div>
        </section>
      </div>
    </SheetContent>
  </Sheet>
</template>
