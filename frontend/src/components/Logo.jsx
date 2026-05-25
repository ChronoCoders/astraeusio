// Astraeusio mark — sun + orbit + alert arc. Transparent, scales to any size.
// Tuned for dark headers (amber sun, faint white orbit). Geometry matches the
// canonical brand mark centered at (50,50) in a 100×100 box.
export default function Logo({ size = 22, className = '' }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 100 100"
      className={className}
      role="img"
      aria-label="Astraeusio"
    >
      <circle cx="50" cy="50" r="29" fill="none" stroke="rgba(255,255,255,0.18)" strokeWidth="0.8" />
      <path
        d="M 55.0 21.4 A 29 29 0 0 1 78.6 45.0"
        fill="none"
        stroke="#D97706"
        strokeWidth="2.2"
        strokeLinecap="round"
        opacity="0.75"
      />
      <circle cx="50" cy="50" r="11" fill="#D97706" />
      <circle cx="50" cy="50" r="6.5" fill="#FAC775" opacity="0.4" />
    </svg>
  )
}
