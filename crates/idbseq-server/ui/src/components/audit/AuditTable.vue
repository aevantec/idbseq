<script setup lang="ts">
import { Badge } from '@/components/ui/badge';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import { formatTime, summarizeChange, type ChangeRow } from '@/lib/format';
import type { AuditEntry } from '@/lib/types';

const SHOWN_CHANGES = 2;

defineProps<{ entries: AuditEntry[]; showCollection?: boolean; emptyText?: string }>();
const emit = defineEmits<{ select: [entry: AuditEntry] }>();

function preview(entry: AuditEntry): { rows: ChangeRow[]; more: number } {
  const rows = summarizeChange(entry.previous, entry.new);
  return { rows: rows.slice(0, SHOWN_CHANGES), more: Math.max(0, rows.length - SHOWN_CHANGES) };
}

function tone(action: string): 'destructive' | 'secondary' | 'outline' {
  if (action.endsWith('.delete') || action.endsWith('.revoke') || action.endsWith('_failed') || action.endsWith('.reset')) return 'destructive';
  return action.startsWith('auth.') ? 'outline' : 'secondary';
}
</script>

<template>
  <Table>
    <TableHeader>
      <TableRow>
        <TableHead>When</TableHead>
        <TableHead>Action</TableHead>
        <TableHead v-if="showCollection">Collection</TableHead>
        <TableHead>Change</TableHead>
        <TableHead>Who</TableHead>
      </TableRow>
    </TableHeader>
    <TableBody>
      <TableRow v-if="entries.length === 0">
        <TableCell :colspan="showCollection ? 5 : 4" class="py-8 text-center text-muted-foreground">{{ emptyText ?? 'No entries.' }}</TableCell>
      </TableRow>
      <TableRow v-for="entry in entries" :key="entry.id" class="cursor-pointer" @click="emit('select', entry)">
        <TableCell class="whitespace-nowrap">
          <button type="button" class="text-left hover:underline focus-visible:underline focus-visible:outline-none" @click.stop="emit('select', entry)">
            {{ formatTime(entry.at) }}
          </button>
        </TableCell>
        <TableCell><Badge :variant="tone(entry.action)" class="font-mono">{{ entry.action }}</Badge></TableCell>
        <TableCell v-if="showCollection">{{ entry.collection ?? '—' }}</TableCell>
        <TableCell class="max-w-md">
          <template v-if="preview(entry).rows.length > 0">
            <p v-for="row in preview(entry).rows" :key="row.field" class="truncate font-mono text-xs">
              {{ row.field }}: <template v-if="row.kind === 'changed'">{{ row.before }} → </template>{{ row.kind === 'removed' ? row.before : row.after }}
            </p>
            <p v-if="preview(entry).more > 0" class="text-xs text-muted-foreground">+{{ preview(entry).more }} more</p>
          </template>
          <span v-else class="text-muted-foreground">—</span>
        </TableCell>
        <TableCell class="whitespace-nowrap">{{ entry.actor }}</TableCell>
      </TableRow>
    </TableBody>
  </Table>
</template>
