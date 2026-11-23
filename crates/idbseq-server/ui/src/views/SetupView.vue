<script setup lang="ts">
import { computed, onMounted, reactive, ref, useTemplateRef } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import { toast } from 'vue-sonner';

import FormField from '@/components/common/FormField.vue';
import InlineMessage from '@/components/common/InlineMessage.vue';
import BrandMark from '@/components/layout/BrandMark.vue';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { ApiError, describeError } from '@/lib/api';
import { MIN_PASSWORD_LEN, validateNewPassword } from '@/lib/password';
import { api, signIn } from '@/session';

const route = useRoute();
const router = useRouter();
const token = computed(() => (typeof route.query.token === 'string' ? route.query.token : ''));
const form = reactive({ password: '', confirm: '' });
const errors = ref<string[]>([]);
const isAdminPresent = ref(false);
const isBusy = ref(false);
const passwordInput = useTemplateRef<{ $el: HTMLInputElement }>('passwordInput');

onMounted(() => passwordInput.value?.$el.focus());

function explain(error: unknown): string {
  if (!(error instanceof ApiError)) return describeError(error);
  if (error.code === 'UNAUTHORIZED') {
    return 'This setup link is invalid, already used or expired. Restart idbseqd to print a new one.';
  }
  if (error.code === 'NOT_FOUND') return 'First-run setup is not available: the console is off on this server.';
  return describeError(error);
}

async function submit(): Promise<void> {
  errors.value = validateNewPassword(form.password, form.confirm);
  if (errors.value.length > 0) return;
  isBusy.value = true;
  try {
    const { data } = await api.setup(token.value, form.password);
    Object.assign(form, { password: '', confirm: '' });
    signIn(data);
    toast.success('Admin account created. You are signed in.');
    await router.replace({ name: 'collections' });
  } catch (error) {
    isAdminPresent.value = error instanceof ApiError && error.code === 'FORBIDDEN';
    errors.value = [isAdminPresent.value ? 'An admin account already exists.' : explain(error)];
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
          <h1 class="text-xl font-semibold">Set up idbseq</h1>
          <p class="text-sm text-muted-foreground">Choose the password for the admin account</p>
        </div>
      </div>
      <InlineMessage v-if="!token">
        This page needs the setup link that idbseqd prints when it first starts with the console on.
        Open that link, or run <code class="font-mono">idbseqd admin init</code> on the server.
      </InlineMessage>
      <form v-else class="grid gap-4 rounded-xl border bg-card p-6 shadow-sm" novalidate @submit.prevent="submit">
        <InlineMessage v-if="errors.length > 0">
          <p v-for="message in errors" :key="message">{{ message }}</p>
          <RouterLink v-if="isAdminPresent" :to="{ name: 'login' }" class="underline">Sign in instead</RouterLink>
        </InlineMessage>
        <FormField v-slot="{ id }" label="Username">
          <Input :id="id" model-value="admin" readonly autocomplete="username" class="text-muted-foreground" />
        </FormField>
        <FormField v-slot="{ id, describedby }" label="Password" :hint="`At least ${MIN_PASSWORD_LEN} characters. A passphrase of a few words is easy to type and hard to guess.`">
          <Input :id="id" ref="passwordInput" v-model="form.password" type="password" autocomplete="new-password" :aria-describedby="describedby" />
        </FormField>
        <FormField v-slot="{ id }" label="Confirm password">
          <Input :id="id" v-model="form.confirm" type="password" autocomplete="new-password" />
        </FormField>
        <Button type="submit" :disabled="isBusy" class="w-full">Create admin and sign in</Button>
        <p class="text-xs text-muted-foreground">
          The link works once. Change the password later under Account, or reset it with
          <code class="font-mono">idbseqd admin reset-password</code> on the server.
        </p>
      </form>
    </div>
  </main>
</template>
