<script setup lang="ts">
// `idbseqd console <file>` links here with ?token=…&file=…: sign in with the
// token, then move on, so the token leaves the address bar.
import { onMounted, ref } from 'vue';
import { useRoute, useRouter } from 'vue-router';

import InlineMessage from '@/components/common/InlineMessage.vue';
import BrandMark from '@/components/layout/BrandMark.vue';
import { api, LOCAL_ENDED, session, signInLocal, takeNotice } from '@/session';

const route = useRoute();
const router = useRouter();
const problem = ref<string | null>(takeNotice());

const NO_TOKEN = 'Open the link idbseqd console printed. Reloading the page signs you out.';
const NOT_A_CONSOLE =
  'This link only works with idbseqd console <file>. On a server, sign in with the admin password.';

function single(value: unknown): string {
  return typeof value === 'string' ? value : '';
}

/** Only `idbseqd console` accepts a link token as the admin. */
async function isConsole(): Promise<boolean> {
  try {
    return (await api.health()).data.mode === 'console';
  } catch {
    return false;
  }
}

onMounted(async () => {
  const token = single(route.query.token);
  if (token === '') {
    problem.value ??= session.file === null ? NO_TOKEN : LOCAL_ENDED;
    return;
  }
  if (!(await isConsole())) {
    await router.replace({ name: 'local' }); // the token leaves the address bar
    problem.value = NOT_A_CONSOLE;
    return;
  }
  signInLocal(token, single(route.query.file) || 'idbseq file');
  try {
    // Checks the token; a 401 ends the session and lands back here with a notice.
    await api.listCollections({ limit: 1 });
  } catch {
    if (session.token === null) {
      problem.value = LOCAL_ENDED;
      await router.replace({ name: 'local' });
      return;
    }
    // Signed in, but the file is busy or unreadable: the collections page says
    // so and can retry. Either way, the token leaves the address bar.
  }
  await router.replace({ name: 'collections' });
});
</script>

<template>
  <main class="flex min-h-dvh items-center justify-center bg-sidebar p-4">
    <div class="w-full max-w-sm">
      <div class="mb-6 flex flex-col items-center gap-3 text-center">
        <BrandMark class="size-12" />
        <div>
          <h1 class="text-xl font-semibold">idbseq console</h1>
          <p class="text-sm text-muted-foreground">{{ session.file ?? 'A file on this machine' }}</p>
        </div>
      </div>
      <div class="grid gap-4 rounded-xl border bg-card p-6 shadow-sm">
        <InlineMessage v-if="problem">{{ problem }}</InlineMessage>
        <RouterLink v-if="problem === NOT_A_CONSOLE" :to="{ name: 'login' }" class="text-sm underline">Sign in with the password</RouterLink>
        <p v-else class="text-sm text-muted-foreground">Opening the file…</p>
        <p class="text-xs text-muted-foreground">
          Run <code class="font-mono">idbseqd console &lt;file&gt;</code> on this machine and open the link it prints.
        </p>
      </div>
    </div>
  </main>
</template>
