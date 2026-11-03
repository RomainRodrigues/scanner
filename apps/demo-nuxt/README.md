# Nuxt 4 demo

Demo app of the `@gueshbzh-perso/scanner` SDK, linked from `packages/scanner-web`.
`app/components/BarcodeScanner.vue` is a reusable wrapper you can copy into a real app.

Live version: https://romainrodrigues.github.io/scanner/ (deployed by `.github/workflows/pages.yml`).

## First run

From the repository root (npm workspaces):

```bash
npm install
npm run build     # builds the SDK (Rust → WASM + TypeScript); needs Rust, installs the rest
npm run dev       # http://localhost:3000
```

Re-run `npm run build` (or `npm run sdk` from this folder) after changing the Rust
engine or the SDK sources.

## Test on your phone (Cloudflare quick tunnel)

The camera requires HTTPS on a phone. Instead of `npm run dev`, run:

```bash
npm run tunnel    # = nuxt dev --tunnel
```

Nuxt opens a Cloudflare quick tunnel and prints a `https://<random>.trycloudflare.com`
URL plus a QR code: scan it with your phone. The first time, Nuxt asks you to accept
the Cloudflare terms and downloads `cloudflared` itself (or install it beforehand,
e.g. `brew install cloudflared`).

If hot reload misbehaves through the tunnel, use the production build instead:

```bash
npm run preview -w scanner-demo-nuxt   # nuxt build + node .output/server (port 3000)
```
