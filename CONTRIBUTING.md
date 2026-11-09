# Contributing

Thanks for your interest! Bug reports with a **photo of the code that fails**
are the most useful contribution: every real image that does not scan becomes
a test fixture.

## Project layout

```
crates/scanner-core/     Rust engine: decoding strategies, preprocessing, real-time session
crates/scanner-wasm/     wasm-bindgen bindings (low-level API, TypeScript types via tsify)
packages/scanner-web/    TypeScript SDK published on npm (@gueshbzh-perso/scanner)
apps/demo-nuxt/          Nuxt demo (deployed to GitHub Pages)
tests/fixtures/          Committed test images
```

## Setup

Requirements: Rust (stable), Node.js 20+ (the CI uses 22), and for the
end-to-end tests ffmpeg and Chromium.

```bash
npm install
npm run build        # Rust → WebAssembly + TypeScript
npm run dev          # demo on http://localhost:3000
npm run tunnel       # demo + Cloudflare tunnel, to test on a phone
```

## Checks

Every change must pass, without warnings:

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --release
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --workspace
npm run lint && npm run typecheck
npm test             # SDK unit tests (after the build)
npm run test:e2e     # headless Chromium with a fake camera
```

The CI runs all of them on every pull request.

## Guidelines

- **Tested**: a fixed bug comes with a test that fails without the fix. No
  test may skip silently (a missing fixture is a failure).
- **Documented**: public APIs are documented (`missing_docs` in Rust, JSDoc in
  TypeScript). Any non-obvious behaviour (dependency bug workaround, magic
  value, platform limit) is explained in a comment where it happens, with the
  reason or the measurement behind it.
- **Measured**: performance changes give before / after numbers in the commit.
- No `unwrap()` outside tests, no unjustified `unsafe`, no `any` or `!` in
  TypeScript.
- Code and comments in English.

## Commits and releases

Commit messages follow [Conventional Commits](https://www.conventionalcommits.org/)
(`feat:`, `fix:`, `perf:`, `docs:`, `ci:`, `build:`, `chore:`…, `!` for a
breaking change). They **are** the changelog: write them for SDK users.

Releases are automated by release-please: never change version numbers by
hand. Pull requests are merged with a merge commit.
