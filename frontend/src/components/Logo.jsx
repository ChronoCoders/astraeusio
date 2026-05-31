// Astraeusio mark - sun + orbit + alert arc. Transparent, centered at (50,50)
// in a 100×100 box. Strokes are proportionally fat (orbit 2.8, arc 5) so the
// ring and arc stay visible in the 28–48 px header render range - at the old
// thin strokes (0.8/2.2) they went sub-pixel below ~125 px.
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
      <circle cx="50" cy="50" r="32" fill="none" stroke="rgba(255,255,255,0.22)" strokeWidth="2.8" />
      <path
        d="M 55.5 18.5 A 32 32 0 0 1 81.5 44.4"
        fill="none"
        stroke="#D97706"
        strokeWidth="5"
        strokeLinecap="round"
        opacity="0.72"
      />
      <circle cx="50" cy="50" r="14" fill="#D97706" />
      <circle cx="50" cy="50" r="8" fill="#FAC775" opacity="0.45" />
    </svg>
  )
}
