// Demo of the scanner SDK in Nuxt 4.
// The camera only works in a secure context: http://localhost, or https through
// a tunnel (see README: `npm run tunnel` = `nuxt dev --tunnel`, Cloudflare quick tunnel).
import { execSync } from 'node:child_process';
import { readFileSync } from 'node:fs';

// Version shown in the demo footer, resolved at build time. The SDK version only
// changes when a release PR is merged, so the commit is shown too: a demo built
// from main between two releases runs newer code than its version number says.
const sdk = JSON.parse(
  readFileSync(new URL('../../packages/scanner-web/package.json', import.meta.url), 'utf8'),
) as { version: string };

function buildCommit(): string {
  if (process.env.GITHUB_SHA) return process.env.GITHUB_SHA;
  try {
    return execSync('git rev-parse HEAD', { stdio: ['ignore', 'pipe', 'ignore'] })
      .toString()
      .trim();
  } catch {
    return '';
  }
}

export default defineNuxtConfig({
  compatibilityDate: '2025-07-15',
  // Camera scanning is client-only: render the demo as a SPA.
  ssr: false,
  devtools: { enabled: false },
  runtimeConfig: {
    public: {
      sdkVersion: sdk.version,
      commit: buildCommit(),
      repository: process.env.GITHUB_REPOSITORY ?? 'RomainRodrigues/scanner',
    },
  },
  app: {
    head: {
      title: 'Scanner — Nuxt demo',
      meta: [{ name: 'viewport', content: 'width=device-width, initial-scale=1, viewport-fit=cover' }],
    },
  },
  vite: {
    // The SDK loads its worker and .wasm with `new URL(..., import.meta.url)`.
    // Pre-bundling would move the JS away from those files: keep it excluded.
    optimizeDeps: { exclude: ['@gueshbzh-perso/scanner'] },
    server: {
      // Accept requests coming through a Cloudflare quick tunnel.
      allowedHosts: ['.trycloudflare.com'],
      // The SDK is linked from ../../packages/scanner-web.
      fs: { allow: ['../..'] },
    },
  },
});
