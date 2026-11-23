<script setup lang="ts">
import { LOGO_GLYPHS, LOGO_RADIUS, LOGO_SIZE } from '@/lib/brand';
</script>

<template>
  <!-- White on black; inverted in dark mode, where a black square would vanish. -->
  <svg :viewBox="`0 0 ${LOGO_SIZE} ${LOGO_SIZE}`" role="img" aria-label="idbseq">
    <rect :width="LOGO_SIZE" :height="LOGO_SIZE" :rx="LOGO_RADIUS" class="fill-logo" />
    <path :d="LOGO_GLYPHS" class="fill-logo-foreground" />
  </svg>
</template>
