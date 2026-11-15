import { useEffect, useRef, useState } from "react"

import { frameOf } from "./frames"
import type { Frame } from "./scan-client"

/** How often a camera frame goes to the core. */
export const FRAME_MS = 300

interface CameraViewProps {
  /** Called with each frame; resolve `true` to stop (a code was read). */
  onFrame: (frame: Frame) => Promise<boolean>
  onError: (words: string) => void
  label: string
}

/**
 * The camera's picture through the webview's own media APIs
 * (`getUserMedia`), with a frame sent to the core every 300 ms until the
 * core reads a code. The stream stops when this unmounts or a code is
 * read; nothing is recorded or kept.
 */
export function CameraView({ onFrame, onError, label }: CameraViewProps) {
  const video = useRef<HTMLVideoElement>(null)
  const [live, setLive] = useState(false)
  useEffect(() => {
    let stream: MediaStream | null = null
    let timer: ReturnType<typeof setTimeout> | undefined
    let stopped = false
    const stop = () => {
      stopped = true
      clearTimeout(timer)
      stream?.getTracks().forEach((t) => t.stop())
      setLive(false)
    }
    const tick = async () => {
      const v = video.current
      if (stopped || !v) return
      const frame = frameOf(v, v.videoWidth, v.videoHeight)
      if (frame && (await onFrame(frame).catch(() => false))) {
        stop()
        return
      }
      if (!stopped) timer = setTimeout(() => void tick(), FRAME_MS)
    }
    if (!navigator.mediaDevices?.getUserMedia) {
      onError("This window can't use a camera. Open the code's picture as a file, or use your phone.")
      return
    }
    navigator.mediaDevices
      .getUserMedia({ video: { facingMode: "environment" }, audio: false })
      .then(async (s) => {
        if (stopped) {
          s.getTracks().forEach((t) => t.stop())
          return
        }
        stream = s
        const v = video.current
        if (!v) return
        v.srcObject = s
        await v.play()
        setLive(true)
        void tick()
      })
      .catch(() => onError("Lockdown couldn't use the camera. Allow it in this computer's privacy settings, or open the code's picture as a file."))
    return stop
  }, [onFrame, onError])
  // The camera view (an app-level piece, undrawn §2): a square raised panel with a hairline,
  // and one status line under it.
  return (
    <div className="flex w-full max-w-sm flex-col items-center gap-3">
      <video ref={video} muted playsInline aria-label={label} className="bg-surface-raised rule-box aspect-square w-full rounded object-cover" />
      <p className="type-caption text-text-secondary" role="status">
        {live ? "Hold the code steady in the square." : "Starting the camera…"}
      </p>
    </div>
  )
}
