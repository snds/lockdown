import type { HttpAnswer, HttpMeta } from "./core"

const encoder = new TextEncoder()

/** Why there was no answer, as the core reports it (status 0). */
function none(why: string): HttpAnswer {
  return { status: 0, body: encoder.encode(why) }
}

/**
 * The core's HTTP client: a synchronous `XMLHttpRequest` (the core waits
 * for its answer; a dedicated worker may block), to this page's own origin
 * and nowhere else, whatever URL the core was handed. The Content Security
 * Policy says the same (`connect-src 'self'`); this check fails first and
 * in words.
 */
export function sameOriginSend(origin: string) {
  return (meta: HttpMeta, body: Uint8Array): HttpAnswer => {
    let url: URL
    try {
      url = new URL(meta.url)
    } catch {
      return none("the core asked for an address that isn't one")
    }
    if (url.origin !== origin) {
      return none(`the web vault only talks to the server it came from (${origin}), not ${url.origin}`)
    }
    if (!["GET", "PUT", "POST", "HEAD"].includes(meta.method)) {
      return none(`the web vault doesn't send ${meta.method} requests`)
    }
    const xhr = new XMLHttpRequest()
    xhr.open(meta.method, url.href, false)
    xhr.responseType = "arraybuffer"
    xhr.withCredentials = false
    xhr.setRequestHeader("Content-Type", meta.contentType)
    if (meta.authorization) xhr.setRequestHeader("Authorization", meta.authorization)
    try {
      xhr.send(meta.method === "GET" || meta.method === "HEAD" ? null : body.slice())
    } catch {
      return none(`couldn't reach ${origin}`)
    }
    if (xhr.status === 0) return none(`couldn't reach ${origin}`)
    const response = xhr.response as ArrayBuffer | null
    return { status: xhr.status, body: response ? new Uint8Array(response) : new Uint8Array(0) }
  }
}
