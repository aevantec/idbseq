<script setup lang="ts">
import { ExternalLink, KeyRound } from '@lucide/vue';
import { computed, ref } from 'vue';

import CodeBlock from '@/components/common/CodeBlock.vue';
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from '@/components/ui/sheet';
import { buildApiPreview, type HttpMethod } from '@/lib/apiPreview';
import { cn } from '@/lib/utils';
import type { Collection } from '@/lib/types';
import { session } from '@/session';

const props = defineProps<{ collection: Collection }>();
const open = defineModel<boolean>('open', { required: true });

const DOCS = 'https://aevantec.github.io/idbseq';
const docs = computed(() =>
  session.file === null
    ? { href: `${DOCS}/clients/node`, label: 'Node.js SDK docs' }
    : { href: `${DOCS}/guide/embedded`, label: 'Embedding docs' },
);
const METHOD_CLASS: Record<HttpMethod, string> = {
  GET: 'bg-sky-500/15 text-sky-700 dark:text-sky-300',
  POST: 'bg-emerald-500/15 text-emerald-700 dark:text-emerald-300',
  PATCH: 'bg-amber-500/15 text-amber-700 dark:text-amber-300',
  DELETE: 'bg-destructive/15 text-destructive',
};

// The console is served by the API server itself, so its origin is the API's URL.
const operations = computed(() =>
  buildApiPreview(props.collection, session.file === null ? { url: window.location.origin } : { file: session.file }),
);
const selectedId = ref('next');
const selected = computed(() => operations.value.find((op) => op.id === selectedId.value) ?? operations.value[0]);
</script>

<template>
  <Sheet v-model:open="open">
    <SheetContent class="w-full overflow-y-auto sm:max-w-4xl">
      <SheetHeader>
        <SheetTitle>API preview</SheetTitle>
        <SheetDescription>
          SDK code for <span class="font-mono">{{ collection.name }}</span>, with responses from its current values.
        </SheetDescription>
      </SheetHeader>
      <div class="grid gap-6 px-4 pb-6 md:grid-cols-[12rem_minmax(0,1fr)]">
        <nav aria-label="Operations">
          <ul class="flex flex-wrap gap-1 md:flex-col">
            <li v-for="op in operations" :key="op.id">
              <button
                type="button"
                :aria-current="op.id === selected?.id ? 'true' : undefined"
                :class="cn('flex w-full items-center justify-between gap-2 rounded-md px-2.5 py-1.5 text-left text-sm hover:bg-accent', op.id === selected?.id && 'bg-accent font-medium')"
                @click="selectedId = op.id"
              >
                {{ op.title }}
                <span :class="cn('rounded px-1 font-mono text-[10px] font-semibold', METHOD_CLASS[op.method])">{{ op.method }}</span>
              </button>
            </li>
          </ul>
        </nav>

        <section v-if="selected" class="grid min-w-0 content-start gap-4" aria-labelledby="api-op-title">
          <div class="grid gap-1">
            <h3 id="api-op-title" class="font-semibold">{{ selected.title }}</h3>
            <p class="text-sm text-muted-foreground">{{ selected.summary }}</p>
          </div>
          <div class="flex items-start gap-2 rounded-md border px-3 py-2">
            <span :class="cn('rounded px-1.5 py-0.5 font-mono text-xs font-semibold', METHOD_CLASS[selected.method])">{{ selected.method }}</span>
            <code class="font-mono text-sm break-all">{{ selected.path }}</code>
          </div>
          <p class="flex items-start gap-2 text-sm text-muted-foreground">
            <KeyRound class="mt-0.5 size-4 shrink-0" aria-hidden="true" />
            <span>
              {{ selected.access }}
              <RouterLink v-if="session.file === null" :to="{ name: 'keys' }" class="underline hover:text-foreground">Create a key</RouterLink>
            </span>
          </p>

          <div class="grid gap-2">
            <div class="flex flex-wrap items-center justify-between gap-2">
              <h4 class="text-sm font-semibold">Node.js</h4>
              <a :href="docs.href" target="_blank" rel="noreferrer" class="inline-flex items-center gap-1 text-sm text-muted-foreground hover:text-foreground">
                {{ docs.label }} <ExternalLink class="size-3.5" aria-hidden="true" />
              </a>
            </div>
            <CodeBlock :code="selected.code" language="node" label="Node.js code" />
          </div>

          <div class="grid gap-2">
            <h4 class="text-sm font-semibold">Response</h4>
            <CodeBlock :code="selected.response" language="json" label="response" />
          </div>
        </section>
      </div>
    </SheetContent>
  </Sheet>
</template>
