<script setup lang="ts">
import BaseLayout from "@voidzero-dev/vitepress-theme/src/layouts/VPLayout.vue";

import RecordHeader from "../components/RecordHeader.vue";
</script>

<template>
    <BaseLayout>
        <template #doc-before>
            <RecordHeader />
        </template>
    </BaseLayout>
</template>
