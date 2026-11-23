<script setup lang="ts">
import { onMounted, ref } from 'vue';

import RefreshButton from '@/components/common/RefreshButton.vue';
import PageHeader from '@/components/layout/PageHeader.vue';
import { describeError } from '@/lib/api';
import { api } from '@/session';

interface Check {
  label: string;
  isOk: boolean;
  detail: string;
}

const checks = ref<Check[]>([]);

async function outcome(label: string, task: () => Promise<string>): Promise<Check> {
  try {
    return { label, isOk: true, detail: await task() };
  } catch (error) {
    return { label, isOk: false, detail: describeError(error) };
  }
}

async function check(): Promise<void> {
  checks.value = await Promise.all([
    outcome('Health (/healthz)', async () => (await api.health()).data.status),
    outcome('Readiness (/readyz)', async () => (await api.ready()).data.status),
    outcome('API version', async () => String((await api.contract()).data?.info?.version ?? 'unknown')),
  ]);
}

onMounted(check);
</script>

<template>
  <PageHeader :crumbs="[{ label: 'Status' }]">
    <template #actions>
      <RefreshButton @click="check" />
    </template>
  </PageHeader>
  <div class="flex-1 overflow-y-auto">
    <div class="mx-auto grid max-w-4xl gap-4 p-6">
      <dl class="summary grid gap-4 sm:grid-cols-3">
        <div v-for="item in checks" :key="item.label" class="rounded-lg border bg-card p-4">
          <dt class="text-xs font-medium text-muted-foreground">{{ item.label }}</dt>
          <dd class="mt-1 flex items-center gap-2 font-semibold">
            <span :class="['size-2.5 shrink-0 rounded-full', item.isOk ? 'bg-success' : 'bg-destructive']" aria-hidden="true" />
            <span class="break-all">{{ item.detail }}</span>
          </dd>
        </div>
      </dl>
      <p class="text-sm text-muted-foreground">
        Point your orchestrator at /healthz (liveness) and /readyz (readiness), and Prometheus at /metrics. This page is a quick manual check.
      </p>
    </div>
  </div>
</template>
