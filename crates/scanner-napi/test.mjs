#!/usr/bin/env node
/**
 * Test script for @scanner/native
 *
 * Run after building the native addon:
 *   cargo build --release -p scanner-napi
 *   # (or: napi build --platform --release from crates/scanner-napi/)
 *   node test.mjs
 */

import { createRequire } from 'module'
import { readFileSync, existsSync } from 'fs'
import path from 'path'
import { fileURLToPath } from 'url'

const __dirname = path.dirname(fileURLToPath(import.meta.url))
const require = createRequire(import.meta.url)

let Scanner, ScanSession, PixelFormat

try {
  ;({ Scanner, ScanSession, PixelFormat } = require('./index.js'))
  console.log('✓ Native addon loaded\n')
} catch (e) {
  console.error('✗ Failed to load native addon:', e.message)
  console.error('  Build first: cargo build --release -p scanner-napi')
  process.exit(1)
}

let pass = 0
let fail = 0

function test(name, fn) {
  try {
    const result = fn()
    if (result instanceof Promise) {
      return result
        .then(() => { console.log(`  ✓ ${name}`); pass++ })
        .catch(e => { console.error(`  ✗ ${name}: ${e.message}`); fail++ })
    }
    console.log(`  ✓ ${name}`)
    pass++
  } catch (e) {
    console.error(`  ✗ ${name}: ${e.message}`)
    fail++
  }
}

async function main() {
  // ── Constructor tests ────────────────────────────────────────────────────
  console.log('[ Constructor ]')

  await test('Scanner() constructs without error', () => {
    const s = new Scanner()
    if (!s) throw new Error('Got null')
  })

  await test('Scanner({ preprocessing: false }) with config', () => {
    const s = new Scanner({ preprocessing: false, tryRotations: true })
    if (!s) throw new Error('Got null')
  })

  await test('ScanSession() constructs without error', () => {
    const session = new ScanSession()
    if (!session) throw new Error('Got null')
  })

  await test('ScanSession({ roi: { x, y, width, height } }) with ROI', () => {
    const session = new ScanSession({
      roi: { x: 0.2, y: 0.2, width: 0.6, height: 0.6 },
      dedupCooldownMs: 5000,
    })
    if (!session) throw new Error('Got null')
  })

  // ── scanBuffer — blank frame ─────────────────────────────────────────────
  console.log('\n[ scanBuffer ]')

  await test('scanBuffer returns empty array on blank RGBA frame', () => {
    const scanner = new Scanner()
    const buf = Buffer.alloc(100 * 100 * 4, 255) // white
    const results = scanner.scanBuffer(buf, 100, 100, PixelFormat.Rgba)
    if (!Array.isArray(results)) throw new Error('Expected array')
    if (results.length !== 0) throw new Error(`Expected 0 results, got ${results.length}`)
  })

  await test('scanBuffer returns empty array on blank RGB frame', () => {
    const scanner = new Scanner()
    const buf = Buffer.alloc(100 * 100 * 3, 200)
    const results = scanner.scanBuffer(buf, 100, 100, PixelFormat.Rgb)
    if (!Array.isArray(results)) throw new Error('Expected array')
  })

  await test('scanBufferAsync returns empty array on blank frame', async () => {
    const scanner = new Scanner()
    const buf = Buffer.alloc(200 * 200 * 4, 255)
    const results = await scanner.scanBufferAsync(buf, 200, 200, PixelFormat.Rgba)
    if (!Array.isArray(results)) throw new Error('Expected array')
  })

  // ── scanFile ─────────────────────────────────────────────────────────────
  console.log('\n[ scanFile ]')

  const testQr = path.join(__dirname, '../../tests/images/qr/clean_hd.png')

  if (existsSync(testQr)) {
    await test('scanFile decodes a QR code', async () => {
      const scanner = new Scanner()
      const results = await scanner.scanFile(testQr)
      if (!Array.isArray(results)) throw new Error('Expected array')
      if (results.length === 0) throw new Error('Expected at least one result')
      if (!results[0].text) throw new Error('Expected non-empty text')
      console.log(`       → "${results[0].text}" (${results[0].format})`)
    })
  } else {
    console.log('  ~ scanFile QR test: SKIPPED (no test image; run generate_test_images.py)')
  }

  await test('scanFile throws on non-existent file', async () => {
    const scanner = new Scanner()
    try {
      await scanner.scanFile('/tmp/does_not_exist_xyzabc.png')
      throw new Error('Expected scanFile to throw, but it returned')
    } catch (e) {
      if (e.message === 'Expected scanFile to throw, but it returned') throw e
      // Any other error is expected — file not found is correct behavior
    }
  })

  // ── ScanSession ──────────────────────────────────────────────────────────
  console.log('\n[ ScanSession ]')

  await test('processFrame returns null on blank frame', () => {
    const session = new ScanSession()
    const buf = Buffer.alloc(100 * 100 * 4, 255)
    const result = session.processFrame(buf, 100, 100, PixelFormat.Rgba)
    if (result !== null && result !== undefined) {
      throw new Error(`Expected null/undefined, got: ${JSON.stringify(result)}`)
    }
  })

  await test('getStats() returns stats object', () => {
    const session = new ScanSession()
    const buf = Buffer.alloc(100 * 100 * 4, 200)
    session.processFrame(buf, 100, 100)
    const stats = session.getStats()
    if (typeof stats.framesSubmitted !== 'number') throw new Error('Missing framesSubmitted')
    if (typeof stats.avgScanTimeMs !== 'number') throw new Error('Missing avgScanTimeMs')
    if (stats.framesSubmitted < 1) throw new Error('Expected framesSubmitted >= 1')
  })

  await test('reset() clears stats', () => {
    const session = new ScanSession()
    const buf = Buffer.alloc(100 * 100 * 4, 200)
    session.processFrame(buf, 100, 100)
    session.reset()
    const stats = session.getStats()
    if (stats.framesSubmitted !== 0) throw new Error('Expected framesSubmitted=0 after reset')
    if (stats.codesDetected !== 0) throw new Error('Expected codesDetected=0 after reset')
  })

  await test('skipInterval getter returns a number', () => {
    const session = new ScanSession()
    const interval = session.skipInterval
    if (typeof interval !== 'number') throw new Error('Expected number')
    if (interval < 1) throw new Error('Expected interval >= 1')
  })

  await test('clearCooldown() does not throw', () => {
    const session = new ScanSession()
    session.clearCooldown('some-barcode-text')
  })

  // ── Final report ─────────────────────────────────────────────────────────
  console.log(`\n${'─'.repeat(50)}`)
  console.log(`Results: ${pass} passed, ${fail} failed`)

  if (fail > 0) process.exit(1)
}

main().catch(e => {
  console.error('Fatal:', e)
  process.exit(1)
})
