#!/usr/bin/env node
// Minimal static server for the demo (http://localhost:8080/demo/).
// Camera access requires a secure context: localhost qualifies.
import { createServer } from 'node:http';
import { readFile, stat } from 'node:fs/promises';
import { dirname, extname, join, normalize, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const TYPES = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.mjs': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.json': 'application/json',
  '.map': 'application/json',
  '.wasm': 'application/wasm',
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.svg': 'image/svg+xml',
};

export function serve(port = Number(process.env.PORT ?? 8080)) {
  const server = createServer(async (req, res) => {
    try {
      const url = new URL(req.url ?? '/', 'http://localhost');
      let path = normalize(join(root, decodeURIComponent(url.pathname)));
      if (!path.startsWith(root)) throw new Error('forbidden');
      if ((await stat(path)).isDirectory()) path = join(path, 'index.html');
      const body = await readFile(path);
      res.writeHead(200, { 'Content-Type': TYPES[extname(path)] ?? 'application/octet-stream' });
      res.end(body);
    } catch {
      res.writeHead(404).end('Not found');
    }
  });
  return new Promise((ok) => server.listen(port, () => ok(server)));
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const server = await serve();
  console.log(`Demo: http://localhost:${server.address().port}/demo/`);
}
