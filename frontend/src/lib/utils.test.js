// Run with `node --test src/lib/` from frontend/.
//
// Node's own test runner, no vitest and no jsdom. The functions under test are
// pure and the repository already sets "type": "module", so they import
// directly with nothing added to package.json. Adding a test runner to test two
// pure functions would cost more than it proves.

import { test } from 'node:test'
import assert from 'node:assert/strict'

import { currentKp, kp3hPeriodEnd } from './utils.js'

// The fixture is the shape the two endpoints actually return, oldest first,
// with `estimated_kp` on both series. That shared field name is how the two
// pages came to disagree: reading the newest row of either one looks identical
// at the call site.
const ONE_MINUTE = [
  { time_tag: '2026-09-22T02:50:00', estimated_kp: 1.10 },
  { time_tag: '2026-09-22T02:51:00', estimated_kp: 1.20 },
  { time_tag: '2026-09-22T02:52:00', estimated_kp: 1.33 },
]

const THREE_HOUR = [
  { time_tag: '2026-09-21T18:00:00', estimated_kp: 0.00 },
  { time_tag: '2026-09-21T21:00:00', estimated_kp: 5.67 },
]

test('both pages derive the same current Kp from the same series', () => {
  // There is one function, so this is less a test of agreement than a test that
  // the value is the one minute reading and not the three hour one. The two
  // differ by 4.34 Kp in this fixture, which is the difference between quiet
  // and a G1 storm.
  const dashboard = currentKp(ONE_MINUTE)
  const publicSite = currentKp(ONE_MINUTE)

  assert.equal(dashboard, 1.33)
  assert.equal(publicSite, 1.33)
  assert.equal(dashboard, publicSite)
  assert.notEqual(dashboard, THREE_HOUR.at(-1).estimated_kp)
})

test('the newest reading wins, not the largest or the first', () => {
  assert.equal(currentKp([
    { estimated_kp: 8.0 },
    { estimated_kp: 2.0 },
    { estimated_kp: 0.67 },
  ]), 0.67)
})

test('readings at or below zero are absent estimates, not quiet conditions', () => {
  assert.equal(currentKp([{ estimated_kp: 3.0 }, { estimated_kp: 0 }]), 3.0)
  assert.equal(currentKp([{ estimated_kp: 3.0 }, { estimated_kp: -1 }]), 3.0)
  assert.equal(currentKp([{ estimated_kp: 0 }]), null)
})

test('nothing to read is null, never zero', () => {
  // Zero would render as a real Kp of 0 and colour the gauge green, which is a
  // claim about the magnetosphere rather than about the data being missing.
  assert.equal(currentKp([]), null)
  assert.equal(currentKp(null), null)
  assert.equal(currentKp(undefined), null)
})

test('the three hour card shows its own value, not the current one', () => {
  const official = THREE_HOUR.at(-1)
  assert.equal(official.estimated_kp, 5.67)
  assert.notEqual(official.estimated_kp, currentKp(ONE_MINUTE))
})

test('the three hour period is labelled by its end, not its start', () => {
  // NOAA tags the period by its start on 00, 03, 06, 09, 12, 15, 18, 21
  // boundaries. Labelling by the start makes the number look three hours
  // fresher than it is, which is the whole defect.
  assert.equal(kp3hPeriodEnd('2026-09-21T21:00:00'), '00:00 UTC')
  assert.equal(kp3hPeriodEnd('2026-09-21T18:00:00'), '21:00 UTC')
  assert.equal(kp3hPeriodEnd('2026-09-22T09:00:00'), '12:00 UTC')
})

test('an unusable time tag produces no label rather than a wrong one', () => {
  assert.equal(kp3hPeriodEnd(null), null)
  assert.equal(kp3hPeriodEnd(''), null)
  assert.equal(kp3hPeriodEnd('not a date'), null)
})

// The tests above check the rule. This one checks that the pages apply it.
//
// A correct definition nobody calls is the same as no definition, and that is
// how the two pages diverged in the first place: each had its own derivation
// inline. Reading the source is weaker than executing it, and it is what is
// available without a DOM.
test('neither page derives current Kp any way but through this function', async () => {
  const { readFileSync } = await import('node:fs')

  const pages = [
    ['App.jsx', 'src/App.jsx'],
    ['LandingPage.jsx', 'src/components/LandingPage.jsx'],
    ['DashboardPreview.jsx', 'src/components/DashboardPreview.jsx'],
  ]

  // Every way the codebase has spelled "the newest reading in this series".
  // There were four derivations by the time anybody counted: the dashboard, the
  // public site, the sparkline badge and the preview canvas.
  const INLINE = [
    '.at(-1)?.estimated_kp',
    '.at(-1).estimated_kp',
    'series[n-1].estimated_kp',
    'last.estimated_kp',
  ]

  for (const [name, path] of pages) {
    const flat = readFileSync(path, 'utf8').replace(/\s+/g, '')

    // A floor, because a scan that reads nothing finds nothing and passes.
    assert.ok(flat.length > 2000, `read only ${flat.length} characters of ${name}`)

    for (const shape of INLINE) {
      assert.equal(
        flat.includes(shape), false,
        `${name} derives current Kp inline as ${shape} instead of calling currentKp`,
      )
    }
    assert.ok(
      flat.includes('currentKp'),
      `${name} does not call currentKp at all`,
    )
  }
})
