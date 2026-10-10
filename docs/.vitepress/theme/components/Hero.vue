<template>
    <div class="bento-hero">
        <div class="bento-hero__content">
            <h1 class="bento-hero__title">Bento</h1>
            <p class="bento-hero__tagline">
                A TypeScript runtime that belongs to the application using it. Bento makes execution
                an embeddable capability and development tooling a set of cooperating services
                around it.
            </p>
            <div class="bento-hero__actions">
                <a :href="visionLink" class="bento-hero__button bento-hero__button--primary">
                    Vision
                </a>
                <a :href="architectureLink" class="bento-hero__button"> Architecture </a>
            </div>
        </div>
        <div class="bento-hero__art" aria-hidden="true">
            <img :src="heroImage" alt="" />
        </div>
    </div>
</template>

<script setup lang="ts">
import { withBase } from "vitepress";

import heroImage from "../../../public/bento.png?url";

let visionLink = withBase("/VISION");
let architectureLink = withBase("/ARCHITECTURE");
</script>

<style scoped>
.bento-hero {
    display: grid;
    grid-template-columns: 1fr;
    gap: 0;
    width: 100%;
    min-height: 30rem;
    border-bottom: 1px solid var(--vp-c-divider);
}

@media (min-width: 768px) {
    .bento-hero {
        grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    }
}

.bento-hero__content {
    display: flex;
    flex-direction: column;
    justify-content: center;
    padding: 4rem 2.5rem;
    gap: 1.5rem;
    max-width: 36rem;
}

@media (min-width: 768px) {
    .bento-hero__content {
        padding: 5rem 4rem;
    }
}

.bento-hero__title {
    font-size: 2.5rem;
    font-weight: 700;
    letter-spacing: -0.02em;
    line-height: 1.1;
    color: var(--vp-c-text-1);
    margin: 0;
}

@media (min-width: 768px) {
    .bento-hero__title {
        font-size: 3.25rem;
    }
}

.bento-hero__tagline {
    font-size: 1.125rem;
    line-height: 1.55;
    color: var(--vp-c-text-2);
    max-width: 28rem;
    margin: 0;
}

.bento-hero__actions {
    display: flex;
    gap: 0.75rem;
    margin-top: 1rem;
    flex-wrap: wrap;
}

.bento-hero__button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0.625rem 1.25rem;
    border-radius: 0.5rem;
    font-size: 0.9375rem;
    font-weight: 600;
    text-decoration: none;
    border: 1px solid var(--vp-c-divider);
    color: var(--vp-c-text-1);
    background-color: var(--vp-c-bg-soft);
    transition:
        background-color 0.15s ease,
        border-color 0.15s ease;
}

.bento-hero__button:hover {
    background-color: var(--vp-c-bg-mute);
    border-color: var(--vp-c-divider-2);
}

.bento-hero__button--primary {
    background-color: var(--vp-c-brand-1);
    color: #fff;
    border-color: var(--vp-c-brand-1);
}

.bento-hero__button--primary:hover {
    background-color: var(--vp-c-brand-2);
    border-color: var(--vp-c-brand-2);
    color: #fff;
}

.bento-hero__art {
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 3rem 2.5rem;
    border-top: 1px solid var(--vp-c-divider);
}

@media (min-width: 768px) {
    .bento-hero__art {
        border-top: none;
        border-left: 1px solid var(--vp-c-divider);
    }
}

/* The artwork is a 320px icon; scaling it up blurs it and covering the panel crops it. */
.bento-hero__art img {
    width: min(100%, 20rem);
    height: auto;
    display: block;
}
</style>
