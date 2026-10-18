/**
 * Reading a device link's QR code with the camera, where the browser has
 * `BarcodeDetector` (docs/plan/22-security-depth.md §2). A plain overlay
 * with the camera's picture and a Cancel button; the stream stops the
 * moment a code is read or the person cancels. Without the API the scan
 * button isn't offered and the code is typed instead.
 */

interface Detector {
  detect(source: CanvasImageSource): Promise<Array<{ rawValue: string }>>
}

type DetectorClass = new (options: { formats: string[] }) => Detector

/** A scanner when this browser can read QR codes, else `undefined`. */
export function cameraScanner(win: Window & typeof globalThis = window): (() => Promise<string | null>) | undefined {
  const Barcode = (win as unknown as { BarcodeDetector?: DetectorClass }).BarcodeDetector
  if (!Barcode || !win.navigator.mediaDevices?.getUserMedia) return undefined
  return () => scan(win, new Barcode({ formats: ["qr_code"] }))
}

async function scan(win: Window & typeof globalThis, detector: Detector): Promise<string | null> {
  const doc = win.document
  const stream = await win.navigator.mediaDevices.getUserMedia({ video: { facingMode: "environment" } })
  const overlay = doc.createElement("div")
  overlay.setAttribute("role", "dialog")
  overlay.setAttribute("aria-label", "Scan the QR code")
  overlay.className = "fixed inset-0 z-50 flex flex-col items-center justify-center gap-4 bg-black/80 p-6"
  const video = doc.createElement("video")
  video.muted = true
  video.playsInline = true
  video.className = "max-h-[70vh] max-w-full rounded-lg"
  video.srcObject = stream
  const cancel = doc.createElement("button")
  cancel.type = "button"
  cancel.textContent = "Cancel"
  cancel.className = "rounded-md bg-white px-4 py-2 text-sm text-black"
  overlay.append(video, cancel)
  doc.body.append(overlay)
  let done = false
  const stop = () => {
    done = true
    for (const t of stream.getTracks()) t.stop()
    overlay.remove()
  }
  return new Promise<string | null>((resolve, reject) => {
    cancel.onclick = () => {
      stop()
      resolve(null)
    }
    const tick = () => {
      if (done) return
      detector
        .detect(video)
        .then((found) => {
          if (done) return
          const text = found[0]?.rawValue
          if (text) {
            stop()
            resolve(text)
          } else win.setTimeout(tick, 250)
        })
        .catch((e) => {
          stop()
          reject(e)
        })
    }
    void video.play().then(tick, (e) => {
      stop()
      reject(e)
    })
  })
}
