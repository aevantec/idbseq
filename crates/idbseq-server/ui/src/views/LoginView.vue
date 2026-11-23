<script setup lang="ts">
import { onMounted, ref, useTemplateRef } from 'vue';
import { useRoute, useRouter } from 'vue-router';

import FormField from '@/components/common/FormField.vue';
import InlineMessage from '@/components/common/InlineMessage.vue';
import BrandMark from '@/components/layout/BrandMark.vue';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { ApiError, describeError } from '@/lib/api';
import { safeRedirect } from '@/router';
import { api, signIn, takeNotice } from '@/session';

const route = useRoute();
const router = useRouter();
const password = ref('');
const error = ref<string | null>(null);
const notice = ref<string | null>(takeNotice());
const isBusy = ref(false);
const passwordInput = useTemplateRef<{ $el: HTMLInputElement }>('passwordInput');

onMounted(async () => {
  passwordInput.value?.$el.focus();
  // `idbseqd console` has no password: after a reload (which forgets the
  // session), send the person back to the "open the link again" screen.
  try {
    if ((await api.health()).data.mode === 'console') await router.replace({ name: 'local' });
  } catch {
    // The server is unreachable: signing in will say so.
  }
});

function explain(caught: unknown): string {
  if (!(caught instanceof ApiError)) return describeError(caught);
  if (caught.code === 'UNAUTHORIZED') {
    return 'Wrong password, or the account is locked after repeated failures. Try again later.';
  }
  if (caught.code === 'FORBIDDEN') {
    return 'No admin account yet. Open the setup link idbseqd printed when it started, or run `idbseqd admin init` on the server.';
  }
  return describeError(caught);
}

async function submit(): Promise<void> {
  error.value = null;
  isBusy.value = true;
  try {
    const { data } = await api.login(password.value);
    password.value = '';
    signIn(data);
    await router.replace(safeRedirect(route.query.redirect));
  } catch (caught) {
    error.value = explain(caught);
    passwordInput.value?.$el.select();
  } finally {
    isBusy.value = false;
  }
}
</script>

<template>
  <main class="flex min-h-dvh items-center justify-center bg-sidebar p-4">
    <div class="w-full max-w-sm">
      <div class="mb-6 flex flex-col items-center gap-3 text-center">
        <BrandMark class="size-12" />
        <div>
          <h1 class="text-xl font-semibold">idbseq console</h1>
          <p class="text-sm text-muted-foreground">Sign in with the admin account</p>
        </div>
      </div>
      <form class="grid gap-4 rounded-xl border bg-card p-6 shadow-sm" @submit.prevent="submit">
        <InlineMessage v-if="notice" kind="note">{{ notice }}</InlineMessage>
        <InlineMessage v-if="error">{{ error }}</InlineMessage>
        <FormField v-slot="{ id }" label="Username">
          <Input :id="id" model-value="admin" readonly autocomplete="username" class="text-muted-foreground" />
        </FormField>
        <FormField v-slot="{ id }" label="Password">
          <Input :id="id" ref="passwordInput" v-model="password" type="password" required autocomplete="current-password" />
        </FormField>
        <Button type="submit" :disabled="isBusy" class="w-full">Sign in</Button>
        <p class="text-xs text-muted-foreground">
          The session lasts as long as the server allows (15 minutes by default) and is never stored in the browser.
        </p>
      </form>
    </div>
  </main>
</template>
