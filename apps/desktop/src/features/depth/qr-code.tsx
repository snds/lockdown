/**
 * A QR code drawn from the core's module rows (`lockdown_app::qr`): one
 * SVG path, dark modules only, with the four-module quiet zone the
 * standard asks for. No image and no markup cross the bridge.
 */
export function QrCode({ rows, label, size = 208 }: { rows: string[]; label: string; size?: number }) {
  if (rows.length === 0) return null
  const quiet = 4
  const side = rows.length + quiet * 2
  let d = ""
  rows.forEach((row, y) => {
    for (let x = 0; x < row.length; x++) if (row[x] === "1") d += `M${x + quiet} ${y + quiet}h1v1h-1z`
  })
  return (
    <svg role="img" aria-label={label} width={size} height={size} viewBox={`0 0 ${side} ${side}`} shapeRendering="crispEdges" className="rounded-md bg-white">
      <path d={d} fill="black" />
    </svg>
  )
}
