import { test } from 'node:test'
import assert from 'node:assert/strict'

import { apodCreditLines } from './apodCredit.js'

test('the same name as credit and copyright is one line', () => {
  assert.deepEqual(apodCreditLines('Ignacio Fernández', 'Ignacio Fernández'), [
    { label: 'apod.creditAndCopyright', text: 'Ignacio Fernández' },
  ])
})

test('a different copyright holder gets its own line after the credit', () => {
  assert.deepEqual(apodCreditLines('NASA, ESA', 'A. Name'), [
    { label: 'apod.credit', text: 'NASA, ESA' },
    { label: 'apod.copyright', text: 'A. Name' },
  ])
})

test('either one alone is shown alone', () => {
  assert.deepEqual(apodCreditLines('NASA', null), [{ label: 'apod.credit', text: 'NASA' }])
  assert.deepEqual(apodCreditLines(undefined, 'A. Name'), [{ label: 'apod.copyright', text: 'A. Name' }])
})

test('rows stored before credits existed show no line', () => {
  assert.deepEqual(apodCreditLines(null, null), [])
  assert.deepEqual(apodCreditLines('', ''), [])
})
