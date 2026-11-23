<script setup lang="ts">
import { Check, Copy } from '@lucide/vue';
import { computed, ref, watch } from 'vue';

import { Button } from '@/components/ui/button';
import { tokenize, type Language, type TokenKind } from '@/lib/highlight';

const props = defineProps<{ code: string; language: Language; label: string }>();

// Classes only: tokens are rendered as text, never as HTML, and the CSP allows no inline styles.
const TOKEN_CLASS: Record<TokenKind, string> = {
  plain: '',
  keyword: 'text-violet-700 dark:text-violet-300',
  string: 'text-emerald-700 dark:text-emerald-300',
  number: 'text-amber-700 dark:text-amber-300',
  comment: 'italic text-muted-foreground',
  property: 'text-sky-700 dark:text-sky-300',
};
const COPIED_FOR_MS = 1500;

const tokens = computed(() => tokenize(props.code, props.language));
const isCopied = ref(false);
const copyError = ref<string | null>(null);

watch(() => props.code, () => {
  isCopied.value = false;
  copyError.value = null;
});

async function copy(): Promise<void> {
  try {
    await navigator.clipboard.writeText(props.code);
    isCopied.value = true;
    copyError.value = null;
    setTimeout(() => (isCopied.value = false), COPIED_FOR_MS);
  } catch {
    copyError.value = 'The browser blocked copying. Select the code and press Ctrl+C or ⌘C.';
  }
}
</script>

<template>
  <div class="grid gap-1">
    <div class="relative rounded-md border bg-muted/50">
      <Button
        type="button"
        variant="ghost"
        size="icon"
        class="absolute top-1.5 right-1.5 size-7"
        :aria-label="isCopied ? `${label} copied` : `Copy ${label}`"
        @click="copy"
      >
        <Check v-if="isCopied" class="size-3.5 text-success" aria-hidden="true" />
        <Copy v-else class="size-3.5" aria-hidden="true" />
      </Button>
      <pre class="overflow-x-auto p-4 pr-10 font-mono text-xs leading-relaxed" :aria-label="label"><code><span v-for="(token, index) in tokens" :key="index" :class="TOKEN_CLASS[token.kind]">{{ token.text }}</span></code></pre>
    </div>
    <p v-if="copyError" class="text-xs text-destructive" aria-live="polite">{{ copyError }}</p>
  </div>
</template>
