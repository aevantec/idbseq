<script setup lang="ts">
import { useIntervalFn } from '@vueuse/core';
import { computed, ref } from 'vue';

import AppRail from '@/components/layout/AppRail.vue';
import IdleGuard from '@/components/layout/IdleGuard.vue';
import { session } from '@/session';

const TICK_MS = 1000;
const now = ref(Date.now());
useIntervalFn(() => (now.value = Date.now()), TICK_MS);
const timeLeft = computed(() => (session.expiresAt === null ? null : session.expiresAt - now.value));
</script>

<template>
  <div class="flex h-dvh overflow-hidden">
    <AppRail :time-left="timeLeft" />
    <div class="flex min-w-0 flex-1 flex-col overflow-hidden">
      <RouterView />
    </div>
    <!-- A console on a file has no session to expire. -->
    <IdleGuard v-if="session.file === null" :now="now" />
  </div>
</template>
