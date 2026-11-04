/**
 * The words around the owner's question about someone who reset their
 * account (docs/plan/55-member-key-change.md). The question itself is the
 * core's (`KeyResetView.words`).
 */

/** Under the question: what yes means, so nobody says it by reflex. */
export const RESET_HINT = "Say yes if you know they did. Until then, what you share with them stays closed to them."

/** What the screen says once someone is let back in. */
export function letBackInNotice(login: string): string {
  return `${login} can open what you share with them again.`
}
