// The credit lines shown under the APOD image, as i18n label keys and plain
// text. NASA's feed gave the same name as credit and copyright on every item
// of 2026, so equal values make one line rather than the name twice.
export function apodCreditLines(credit, copyright) {
  if (credit && credit === copyright) {
    return [{ label: 'apod.creditAndCopyright', text: credit }]
  }
  const lines = []
  if (credit) lines.push({ label: 'apod.credit', text: credit })
  if (copyright) lines.push({ label: 'apod.copyright', text: copyright })
  return lines
}
