# Nuxt 4 demo

Demo app of the `@gueshbzh-perso/scanner` SDK, linked from `packages/scanner-web`.
`app/components/BarcodeScanner.vue` is a reusable wrapper you can copy into a real app.

## First run

```bash
cd apps/demo-nuxt
npm run sdk       # builds the SDK (Rust → WASM + TypeScript); needs Rust, installs the rest
npm install
npm run dev       # http://localhost:3000
```

Re-run `npm run sdk` after changing the Rust engine or the SDK sources.

## Test on your phone (Cloudflare quick tunnel)

The camera requires HTTPS on a phone. With [`cloudflared`](https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/downloads/)
installed (`brew install cloudflared` on macOS), in a second terminal:

```bash
npm run tunnel
```

Open the `https://<random>.trycloudflare.com` URL it prints on your phone.
`*.trycloudflare.com` is already allowed in `nuxt.config.ts`.

If hot reload misbehaves through the tunnel, use the production build instead:

```bash
npm run preview   # nuxt build + node .output/server (port 3000)
```
