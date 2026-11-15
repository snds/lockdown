/** Words typed into All items' search field, waiting for Search to open with them (brief V1: typing there opens Search). */
let typed = ""

/** Opens Search with what was typed elsewhere (All items' field): Search starts with these words in its field. */
export function searchWith(words: string): void {
  typed = words
}

/** The words waiting for Search. */
export function typedWords(): string {
  return typed
}

/** Taken: the next Search starts empty. */
export function clearTyped(): void {
  typed = ""
}
