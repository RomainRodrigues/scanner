// Third-party notices for the npm package.
//
// The WebAssembly engine is compiled from ~70 Rust crates (rxing, image,
// serde…), under MIT, Apache-2.0, BSD… licenses that require their notices to
// ship with the binary. This lists every crate linked into the WASM build
// (normal dependencies of scanner-wasm for wasm32, workspace crates excluded)
// and appends the license files found in each crate, deduplicated by content.
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';

const LICENSE_FILE = /^(licen[cs]e|copying|notice|unlicense)/i;

/** Crates linked into the WASM engine, sorted by name. */
function wasmCrates(workspace) {
  const meta = JSON.parse(
    execFileSync(
      'cargo',
      ['metadata', '--format-version', '1', '--filter-platform', 'wasm32-unknown-unknown'],
      {
        cwd: workspace,
        maxBuffer: 256 * 1024 * 1024,
      },
    ).toString(),
  );
  const packages = new Map(meta.packages.map((p) => [p.id, p]));
  const nodes = new Map(meta.resolve.nodes.map((n) => [n.id, n]));
  const members = new Set(meta.workspace_members);
  const root = meta.packages.find((p) => p.name === 'scanner-wasm');
  const seen = new Set();
  const stack = [root.id];
  while (stack.length) {
    const id = stack.pop();
    if (seen.has(id)) continue;
    seen.add(id);
    for (const dep of nodes.get(id).deps) {
      // Normal dependencies only (`kind: null`): dev and build deps are not linked.
      if (dep.dep_kinds.some((k) => k.kind === null)) stack.push(dep.pkg);
    }
  }
  return [...seen]
    .filter((id) => !members.has(id))
    .map((id) => packages.get(id))
    .sort((a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version));
}

function licenseTexts(pkg) {
  const dir = dirname(pkg.manifest_path);
  return readdirSync(dir)
    .filter((f) => LICENSE_FILE.test(f))
    .sort()
    .map((f) => readFileSync(join(dir, f), 'utf8').trim());
}

/** Write the notices to `outFile`. `workspace` is the Cargo workspace root. */
export function writeThirdPartyLicenses(workspace, outFile) {
  const crates = wasmCrates(workspace);
  const texts = new Map(); // hash → { text, users }
  const rows = [];
  for (const pkg of crates) {
    const refs = [];
    for (const text of licenseTexts(pkg)) {
      const hash = createHash('sha256').update(text).digest('hex').slice(0, 12);
      if (!texts.has(hash)) texts.set(hash, { text, users: [] });
      texts.get(hash).users.push(`${pkg.name} ${pkg.version}`);
      refs.push(`[${hash}](#license-${hash})`);
    }
    rows.push(`| ${pkg.name} | ${pkg.version} | ${pkg.license ?? '?'} | ${refs.join(' ') || '—'} |`);
  }

  const out = [
    '# Third-party licenses',
    '',
    'The WebAssembly engine shipped in `dist/wasm/` is compiled from the Rust crates',
    'below. Their license texts follow, each listed once with the crates using it',
    '(crates whose published package has no license file show their SPDX expression only).',
    '',
    '| Crate | Version | License | Texts |',
    '| --- | --- | --- | --- |',
    ...rows,
    '',
  ];
  for (const [hash, { text, users }] of texts) {
    out.push(`## License ${hash}`, '', `<a id="license-${hash}"></a>Used by: ${users.join(', ')}`, '');
    out.push('```text', text, '```', '');
  }
  writeFileSync(outFile, out.join('\n'));
  return { crates: crates.length, texts: texts.size };
}
