<script setup lang="ts">
import { useEventListener } from '@vueuse/core';
import { computed, ref, watch } from 'vue';

import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
import { Button } from '@/components/ui/button';
import { describeError } from '@/lib/api';
import { formatCountdown } from '@/lib/format';
import { WARN_BEFORE_MS, nextIdleAction } from '@/lib/idle';
import { endSession, extendSession, session, signOut } from '@/session';

// Keeps the session alive while the admin is active, asks "Are you still
// there?" a minute before an idle sign-out, and signs out when time runs out.

const props = defineProps<{ now: number }>();

const ACTIVITY_EVENTS = ['pointerdown', 'pointermove', 'keydown', 'wheel', 'touchstart', 'scroll'];

const isWarning = ref(false);
const isAtLimit = ref(false);
const isBusy = ref(false);
const error = ref<string | null>(null);
let lastActivityAt = Date.now();
let lastRefreshAt = Date.now();
let isRefreshing = false;

const timeLeft = computed(() => (session.expiresAt === null ? 0 : session.expiresAt - props.now));

useEventListener(
  window,
  ACTIVITY_EVENTS,
  () => {
    // While the prompt is open, only the admin's choice counts.
    if (!isWarning.value) lastActivityAt = Date.now();
  },
  { passive: true, capture: true },
);

/** After a refresh: the server stops extending 12 hours after sign-in. */
function checkLimit(): void {
  isAtLimit.value = session.expiresAt !== null && session.expiresAt - Date.now() <= WARN_BEFORE_MS;
}

async function refreshQuietly(now: number): Promise<void> {
  if (isRefreshing) return;
  isRefreshing = true;
  lastRefreshAt = now; // also paces retries after a failure
  try {
    await extendSession();
    checkLimit();
  } catch {
    // Nothing to show yet: if the server stays unreachable, the prompt appears
    // a minute before the end and reports the error when the admin answers.
  } finally {
    isRefreshing = false;
  }
}

watch(
  () => props.now,
  (now) => {
    const action = nextIdleAction({
      now,
      expiresAt: session.expiresAt,
      lastActivityAt,
      lastRefreshAt,
      isWarning: isWarning.value,
      isAtLimit: isAtLimit.value,
    });
    if (action === 'refresh') void refreshQuietly(now);
    if (action === 'warn') isWarning.value = true;
    if (action === 'expire') {
      isWarning.value = false;
      endSession(
        isAtLimit.value
          ? 'Your session reached its 12-hour limit. Sign in again.'
          : 'You were signed out after a period of inactivity. Sign in again.',
      );
    }
  },
);

async function stay(): Promise<void> {
  if (isAtLimit.value) return;
  isBusy.value = true;
  error.value = null;
  try {
    await extendSession();
    lastActivityAt = Date.now();
    lastRefreshAt = lastActivityAt;
    checkLimit();
    isWarning.value = isAtLimit.value;
  } catch (caught) {
    error.value = `Could not extend the session: ${describeError(caught)}`;
  } finally {
    isBusy.value = false;
  }
}

async function leave(): Promise<void> {
  isWarning.value = false;
  await signOut();
}

// Escape counts as "I'm here".
function onOpenChange(isOpen: boolean): void {
  if (!isOpen) void stay();
}
</script>

<template>
  <AlertDialog :open="isWarning" @update:open="onOpenChange">
    <AlertDialogContent>
      <AlertDialogHeader>
        <AlertDialogTitle>{{ isAtLimit ? 'Your session is ending' : 'Are you still there?' }}</AlertDialogTitle>
        <AlertDialogDescription v-if="isAtLimit">
          Sessions last at most 12 hours. You will be signed out in
          <span class="font-mono font-medium text-foreground">{{ formatCountdown(timeLeft) }}</span>; sign in again to continue.
        </AlertDialogDescription>
        <AlertDialogDescription v-else>
          There has been no activity for a while, so you will be signed out in
          <span class="font-mono font-medium text-foreground">{{ formatCountdown(timeLeft) }}</span>.
        </AlertDialogDescription>
      </AlertDialogHeader>
      <p v-if="error" role="alert" class="text-sm text-destructive">{{ error }}</p>
      <AlertDialogFooter>
        <Button type="button" variant="outline" @click="leave">Sign out</Button>
        <Button v-if="!isAtLimit" type="button" :disabled="isBusy" autofocus @click="stay">Stay signed in</Button>
      </AlertDialogFooter>
    </AlertDialogContent>
  </AlertDialog>
</template>
