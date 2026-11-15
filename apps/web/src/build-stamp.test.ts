// @vitest-environment node
import { BUILD_META, buildId, buildJson, stampPage } from "../build-stamp"
import { readBuild } from "./core/build-watch"

describe("the build stamp", () => {
  const files: [string, string | Uint8Array][] = [
    ["index.html", "<html><head></head></html>"],
    ["assets/index-abc.js", "console.log(1)"],
    ["assets/lockdown_web-def.wasm", new Uint8Array([0, 97, 115, 109])],
  ]

  it("is the same for the same files, in any order", () => {
    const id = buildId(files)
    expect(id).toMatch(/^[0-9a-f]{20}$/)
    expect(buildId([...files].reverse())).toBe(id)
  })

  it("changes when a file's content or name changes, or a file is added", () => {
    const id = buildId(files)
    expect(buildId([["index.html", "<html><head></head></html>"], ["assets/index-abc.js", "console.log(2)"], files[2]!])).not.toBe(id)
    expect(buildId([files[0]!, ["assets/index-abd.js", "console.log(1)"], files[2]!])).not.toBe(id)
    expect(buildId([...files, ["assets/extra.css", ""]])).not.toBe(id)
    // One file's end is never read as the next one's start.
    expect(buildId([["a", "bc"], ["d", ""]])).not.toBe(buildId([["a", "b"], ["cd", ""]]))
  })

  it("writes a build.json the watch reads back", () => {
    const id = buildId(files)
    expect(readBuild(buildJson(id))).toBe(id)
  })

  it("puts the id in the page's head, where the watch looks", () => {
    const html = stampPage("<!doctype html>\n<html>\n  <head>\n    <title>Lockdown web vault</title>\n  </head>\n  <body></body>\n</html>\n", "0123456789abcdef0123")
    expect(html).toContain(`<meta name="${BUILD_META}" content="0123456789abcdef0123" />`)
    expect(html!.indexOf(BUILD_META)).toBeLessThan(html!.indexOf("</head>"))
    expect(stampPage("<p>no head</p>", "x")).toBeNull()
  })

  it("uses the tag name the page reads", async () => {
    const { readFileSync } = await import("node:fs")
    const dom = readFileSync(new URL("./core/build-watch-dom.ts", import.meta.url), "utf8")
    expect(dom).toContain(`const BUILD_META = "${BUILD_META}"`)
  })
})
