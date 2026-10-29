/** Served by the `iconCodepoints` Vite plugin (scripts/icon-codepoints.ts): each icon the sources name, by its codepoint in the shipped font. */
declare module "virtual:icon-codepoints" {
  const codepoints: Readonly<Record<string, number>>
  export default codepoints
}
