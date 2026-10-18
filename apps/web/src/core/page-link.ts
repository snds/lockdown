/** A link this page was opened for, rebuilt from the address the server sent us to. */
export interface PageLink {
  kind: "share" | "request"
  /** The whole link, its fragment (the key) included. */
  link: string
}

/**
 * The link a `/vault/s/{id}` or `/vault/r/{id}` address stands for: the
 * server redirected `/s/{id}#key` here and the browser kept the fragment
 * (docs/plan/17-web-vault.md §7). `null` for any other address.
 */
export function linkFromLocation(origin: string, pathname: string, hash: string): PageLink | null {
  const m = /^\/vault\/(s|r)\/([0-9A-Fa-f-]{1,64})$/.exec(pathname)
  if (!m || hash.length < 2) return null
  return { kind: m[1] === "s" ? "share" : "request", link: `${origin}/${m[1]}/${m[2]}${hash}` }
}
