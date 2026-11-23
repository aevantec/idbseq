<script setup lang="ts">
import { reactive, ref, watch } from 'vue';

import FormField from '@/components/common/FormField.vue';
import InlineMessage from '@/components/common/InlineMessage.vue';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from '@/components/ui/sheet';
import { ApiError, describeError } from '@/lib/api';
import { MIN_PASSWORD_LEN, validatePasswordChange } from '@/lib/password';
import { api, endSession } from '@/session';

const open = defineModel<boolean>('open', { required: true });

const form = reactive({ current: '', next: '', confirm: '' });
const errors = ref<string[]>([]);
const isBusy = ref(false);

// Never keep passwords around once the panel closes.
watch(open, (isOpen) => {
  if (isOpen) return;
  Object.assign(form, { current: '', next: '', confirm: '' });
  errors.value = [];
});

async function submit(): Promise<void> {
  errors.value = validatePasswordChange(form);
  if (errors.value.length > 0) return;
  isBusy.value = true;
  try {
    await api.changePassword(form.current, form.next);
    open.value = false;
    // The server has revoked every admin session, this one included.
    endSession('Password changed. Sign in with the new password.');
  } catch (error) {
    errors.value = [
      error instanceof ApiError && error.code === 'UNAUTHORIZED'
        ? 'The current password is incorrect, or your session has ended.'
        : describeError(error),
    ];
  } finally {
    isBusy.value = false;
  }
}
</script>

<template>
  <Sheet v-model:open="open">
    <SheetContent class="w-full overflow-y-auto sm:max-w-xl">
      <SheetHeader>
        <SheetTitle>Account</SheetTitle>
        <SheetDescription>Signed in as <span class="font-mono">admin</span>.</SheetDescription>
      </SheetHeader>
      <form class="grid gap-4 px-4 pb-6" novalidate @submit.prevent="submit">
        <h3 class="font-semibold">Change password</h3>
        <InlineMessage v-if="errors.length > 0">
          <p v-for="message in errors" :key="message">{{ message }}</p>
        </InlineMessage>
        <FormField v-slot="{ id }" label="Current password">
          <Input :id="id" v-model="form.current" type="password" autocomplete="current-password" />
        </FormField>
        <FormField v-slot="{ id, describedby }" label="New password" :hint="`At least ${MIN_PASSWORD_LEN} characters. A passphrase of a few words is easy to type and hard to guess.`">
          <Input :id="id" v-model="form.next" type="password" autocomplete="new-password" :aria-describedby="describedby" />
        </FormField>
        <FormField v-slot="{ id }" label="Confirm new password">
          <Input :id="id" v-model="form.confirm" type="password" autocomplete="new-password" />
        </FormField>
        <p class="text-xs text-muted-foreground">
          Changing the password signs out every admin session, this one included, and is recorded in the audit log.
          If the password is lost, run <code class="font-mono">idbseqd admin reset-password</code> on the server.
        </p>
        <div class="flex justify-end">
          <Button type="submit" :disabled="isBusy">Change password</Button>
        </div>
      </form>
    </SheetContent>
  </Sheet>
</template>
