// Demo of the scanner SDK in Nuxt 4.
// The camera only works in a secure context: http://localhost, or https through
// a tunnel (see README: `npm run tunnel` exposes the dev server with Cloudflare).
export default defineNuxtConfig({
  compatibilityDate: '2025-07-15',
  // Camera scanning is client-only: render the demo as a SPA.
  ssr: false,
  devtools: { enabled: false },
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
