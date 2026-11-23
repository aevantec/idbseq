<script setup lang="ts">
import { watch } from 'vue';
import { useRouter } from 'vue-router';

import { Toaster } from '@/components/ui/sonner';
import { TooltipProvider } from '@/components/ui/tooltip';
import { session } from '@/session';

const router = useRouter();

// Whenever the session ends (sign out, expiry, a rejected token), go to login.
watch(
  () => session.token,
  (token) => {
    if (token === null) void router.replace({ name: 'login' });
  },
);
</script>

<template>
  <TooltipProvider :delay-duration="200">
    <RouterView />
  </TooltipProvider>
  <Toaster position="bottom-right" rich-colors close-button />
</template>
