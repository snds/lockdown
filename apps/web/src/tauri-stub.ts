/**
 * The desktop's `core/client.ts` imports Tauri's `invoke` for its own
 * client, which the web vault never uses (it provides `webClient`). The
 * build points that import here, so no Tauri code reaches the browser.
 */
export function invoke<T>(cmd: string, args?: unknown, options?: unknown): Promise<T> {
  void args
  void options
  return Promise.reject({ code: "refused", message: `there is no desktop app behind this page to run ${cmd}` })
}
