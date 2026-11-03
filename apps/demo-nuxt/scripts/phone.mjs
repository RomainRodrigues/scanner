#!/usr/bin/env node
// Starts the Nuxt dev server AND a Cloudflare quick tunnel in one command,
// then prints the https URL to open on the phone. Ctrl+C stops both.
import { spawn, spawnSync } from 'node:child_process';

const PORT = process.env.PORT ?? '3000';

if (spawnSync('cloudflared', ['--version'], { stdio: 'ignore' }).error) {
  console.error(
    'cloudflared is not installed.\n' +
      '  macOS:  brew install cloudflared\n' +
      '  Linux:  https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/downloads/',
  );
  process.exit(1);
}

const children = [];
const stopAll = () => children.forEach((c) => c.kill('SIGTERM'));
process.on('SIGINT', () => (stopAll(), process.exit(0)));
process.on('SIGTERM', () => (stopAll(), process.exit(0)));

const nuxt = spawn('npx', ['nuxt', 'dev', '--port', PORT], { stdio: 'inherit' });
children.push(nuxt);

// cloudflared logs to stderr; we only surface the public URL.
const tunnel = spawn('cloudflared', ['tunnel', '--url', `http://localhost:${PORT}`], { stdio: ['ignore', 'pipe', 'pipe'] });
children.push(tunnel);
let shown = false;
const watch = (chunk) => {
  const url = /https:\/\/[a-z0-9-]+\.trycloudflare\.com/.exec(chunk.toString())?.[0];
  if (url && !shown) {
    shown = true;
    console.log(`\n📱  Open on your phone: ${url}\n    (first load can take a few seconds while Nuxt compiles)\n`);
  }
};
tunnel.stdout.on('data', watch);
tunnel.stderr.on('data', watch);

for (const c of children) {
  c.on('exit', (code) => {
    stopAll();
    process.exit(code ?? 0);
  });
}
