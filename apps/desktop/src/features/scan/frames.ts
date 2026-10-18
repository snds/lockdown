import type { Frame } from "./scan-client"

/** The widest frame sent to the core: enough for a QR code across a room, small for the bridge. */
export const MAX_FRAME_SIDE = 640

/**
 * Grey pixels from RGBA (ITU-R BT.601 luma), the only form a picture takes
 * on its way to the core: the core finds and reads the code, the webview
 * never decodes it.
 */
export function greyFrom(rgba: Uint8ClampedArray, width: number, height: number): Frame {
  const luma = new Uint8Array(width * height)
  for (let i = 0, p = 0; i < luma.length; i++, p += 4) {
    luma[i] = ((rgba[p] ?? 0) * 299 + (rgba[p + 1] ?? 0) * 587 + (rgba[p + 2] ?? 0) * 114) / 1000
  }
  return { width, height, luma }
}

/** The size to draw a `w × h` source at, no side over `max`. */
export function fitted(w: number, h: number, max = MAX_FRAME_SIDE): { width: number; height: number } {
  const scale = Math.min(1, max / Math.max(w, h, 1))
  return { width: Math.max(1, Math.round(w * scale)), height: Math.max(1, Math.round(h * scale)) }
}

/** One frame of a video or an image, drawn small and turned grey. */
export function frameOf(source: CanvasImageSource, w: number, h: number): Frame | null {
  if (!w || !h) return null
  const size = fitted(w, h)
  const canvas = document.createElement("canvas")
  canvas.width = size.width
  canvas.height = size.height
  const ctx = canvas.getContext("2d", { willReadFrequently: true })
  if (!ctx) return null
  ctx.drawImage(source, 0, 0, size.width, size.height)
  const data = ctx.getImageData(0, 0, size.width, size.height)
  const frame = greyFrom(data.data, size.width, size.height)
  data.data.fill(0)
  return frame
}

/** An image file as a grey frame. */
export async function frameOfFile(file: Blob): Promise<Frame | null> {
  const bitmap = await createImageBitmap(file)
  try {
    return frameOf(bitmap, bitmap.width, bitmap.height)
  } finally {
    bitmap.close()
  }
}
