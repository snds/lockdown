import { useState, type KeyboardEvent } from "react"

import { Chip, ChipRemove, Input } from "@unicron/ui"

import { parseTags } from "./org-model"

interface TagsEditorProps {
  id: string
  tags: string[]
  onChange: (tags: string[]) => void
  /** Tags already used elsewhere, offered as the person types. */
  known?: string[]
}

/**
 * Tags as chips with a box to add more: Enter or a comma adds what was
 * typed, a chip's remove takes one off. A slash nests one tag under
 * another (`work/clients`). The core normalizes and checks them.
 */
export function TagsEditor({ id, tags, onChange, known = [] }: TagsEditorProps) {
  const [draft, setDraft] = useState("")
  const add = (text: string) => {
    const more = parseTags(text).filter((t) => !tags.some((x) => x.toLowerCase() === t.toLowerCase()))
    if (more.length) onChange([...tags, ...more])
    setDraft("")
  }
  const onKey = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter" || e.key === ",") {
      e.preventDefault()
      if (draft.trim()) add(draft)
    } else if (e.key === "Backspace" && !draft && tags.length) {
      onChange(tags.slice(0, -1))
    }
  }
  const listId = `${id}-known`
  return (
    <div className="flex flex-col gap-2">
      {tags.length > 0 && (
        <ul aria-label="Tags" className="flex flex-wrap gap-1">
          {tags.map((t) => (
            <li key={t}>
              <Chip variant="secondary">
                #{t}
                <ChipRemove label={`Remove tag ${t}`} onClick={() => onChange(tags.filter((x) => x !== t))} />
              </Chip>
            </li>
          ))}
        </ul>
      )}
      <Input
        id={id}
        value={draft}
        list={known.length ? listId : undefined}
        placeholder="Add a tag and press Enter"
        autoComplete="off"
        onChange={(e) => setDraft(e.target.value)}
        onKeyDown={onKey}
        onBlur={() => draft.trim() && add(draft)}
      />
      {known.length > 0 && (
        <datalist id={listId}>
          {known.map((k) => (
            <option key={k} value={k} />
          ))}
        </datalist>
      )}
    </div>
  )
}
