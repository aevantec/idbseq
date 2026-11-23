<script setup lang="ts">
import { Activity, Database, FileDigit, KeyRound, LogOut, ScrollText, UserRound } from '@lucide/vue';
import { computed, ref } from 'vue';

import AccountSheet from '@/components/account/AccountSheet.vue';

import BrandMark from '@/components/layout/BrandMark.vue';
import { Button } from '@/components/ui/button';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import { formatCountdown } from '@/lib/format';
import { session, signOut } from '@/session';

defineProps<{ timeLeft: number | null }>();

const isAccountOpen = ref(false);

const LINKS = [
  { to: '/collections', label: 'Collections', icon: Database, isServerOnly: false },
  { to: '/keys', label: 'API keys', icon: KeyRound, isServerOnly: true },
  { to: '/audit', label: 'Audit log', icon: ScrollText, isServerOnly: true },
  { to: '/status', label: 'Status', icon: Activity, isServerOnly: false },
];

/** A console on a file (`idbseqd console`) has no keys, audit log or account. */
const isLocal = computed(() => session.file !== null);
const links = computed(() => LINKS.filter((link) => !(isLocal.value && link.isServerOnly)));
</script>

<template>
  <nav aria-label="Console" class="flex w-16 shrink-0 flex-col items-center gap-1 border-r bg-sidebar py-3">
    <RouterLink to="/collections" aria-label="idbseq home" class="mb-4 rounded-lg focus-visible:ring-[3px] focus-visible:ring-ring/50 focus-visible:outline-none">
      <BrandMark class="size-9" />
    </RouterLink>
    <Tooltip v-for="link in links" :key="link.to">
      <TooltipTrigger as-child>
        <RouterLink
          :to="link.to"
          :aria-label="link.label"
          class="flex size-10 items-center justify-center rounded-lg text-muted-foreground transition-colors hover:bg-accent hover:text-foreground focus-visible:ring-[3px] focus-visible:ring-ring/50 focus-visible:outline-none"
          active-class="bg-accent !text-foreground"
        >
          <component :is="link.icon" class="size-5" aria-hidden="true" />
        </RouterLink>
      </TooltipTrigger>
      <TooltipContent side="right">{{ link.label }}</TooltipContent>
    </Tooltip>

    <div v-if="isLocal" class="mt-auto flex flex-col items-center">
      <Tooltip>
        <TooltipTrigger as-child>
          <span class="flex size-10 items-center justify-center rounded-lg text-muted-foreground" tabindex="0" :aria-label="`File: ${session.file}`">
            <FileDigit class="size-5" aria-hidden="true" />
          </span>
        </TooltipTrigger>
        <TooltipContent side="right">{{ session.file }}, on this machine</TooltipContent>
      </Tooltip>
    </div>
    <div v-else class="mt-auto flex flex-col items-center gap-2">
      <span
        v-if="timeLeft !== null"
        class="countdown font-mono text-[11px] text-muted-foreground tabular-nums"
        title="Time left in this admin session"
        :aria-label="`Session ends in ${formatCountdown(timeLeft)}`"
      >{{ formatCountdown(timeLeft) }}</span>
      <Tooltip>
        <TooltipTrigger as-child>
          <Button variant="ghost" size="icon" aria-label="Account" @click="isAccountOpen = true">
            <UserRound class="size-5" aria-hidden="true" />
          </Button>
        </TooltipTrigger>
        <TooltipContent side="right">Account</TooltipContent>
      </Tooltip>
      <Tooltip>
        <TooltipTrigger as-child>
          <Button variant="ghost" size="icon" aria-label="Sign out" @click="signOut">
            <LogOut class="size-5" aria-hidden="true" />
          </Button>
        </TooltipTrigger>
        <TooltipContent side="right">Sign out</TooltipContent>
      </Tooltip>
    </div>
    <AccountSheet v-model:open="isAccountOpen" />
  </nav>
</template>
