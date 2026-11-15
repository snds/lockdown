import { useState, type KeyboardEvent } from "react"

import { Chip, ChipField, ChipRemove, Input } from "@unicron/ui"

import { parseTags } from "./org-model"

interface TagsEditorProps {
  id: string
  tags: string[]
  onChange: (tags: string[]) => void
  /** Tags already used elsewhere, offered as the person types. */
  known?: string[]
  /**
   * The field's own label: the box is then unicron's ChipField (brief I4's Folder and tags). Left
   * out where a `FieldLabel` outside names the box (the item editor), which keeps its chips over a
   * plain input until the editor's lane moves it (ChipField adds a status region of its own).
   */
  label?: string
  /** One line under the box (with `label`). */
  helper?: string
}

/** The same tag in other letters is kept once. */
function unique(tags: string[]): string[] {
  const kept: string[] = []
  for (const t of tags) if (!kept.some((k) => k.toLowerCase() === t.toLowerCase())) kept.push(t)
  return kept
}

/**
 * Tags as chips: Enter or a comma adds what was typed, a chip's remove
 * takes one off, Backspace in the empty box takes the last. A slash nests
 * one tag under another (`work/clients`). The core normalizes and checks
 * them.
 */
export function TagsEditor({ id, tags, onChange, known = [], label, helper }: TagsEditorProps) {
  const listId = `${id}-known`
  const offered = known.length > 0 && (
    <datalist id={listId}>
      {known.map((k) => (
        <option key={k} value={k} />
      ))}
    </datalist>
  )
  if (label) {
    return (
      <>
        <ChipField
          id={id}
          label={label}
          helper={helper}
          value={tags}
          onValueChange={(next) => onChange(unique(next))}
          parse={(text) => parseTags(text)[0] ?? null}
          removeLabel={(t) => `Remove tag ${t}`}
          renderChip={(t, chip) => (
            <Chip variant="outline">
              <span>#{t}</span>
              {!chip.locked && <ChipRemove label={`Remove tag ${t}`} onClick={chip.remove} />}
            </Chip>
          )}
          list={known.length ? listId : undefined}
          placeholder="Add a tag"
          autoComplete="off"
          spellCheck={false}
        />
        {offered}
      </>
    )
  }
  return <TagsInput id={id} tags={tags} onChange={onChange} listId={known.length ? listId : undefined} offered={offered} />
}

function TagsInput({ id, tags, onChange, listId, offered }: { id: string; tags: string[]; onChange: (tags: string[]) => void; listId?: string; offered: React.ReactNode }) {
  const [draft, setDraft] = useState("")
  const add = (text: string) => {
    const next = unique([...tags, ...parseTags(text)])
    if (next.length !== tags.length) onChange(next)
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
        list={listId}
        placeholder="Add a tag and press Enter"
        autoComplete="off"
        onChange={(e) => setDraft(e.target.value)}
        onKeyDown={onKey}
        onBlur={() => draft.trim() && add(draft)}
      />
      {offered}
    </div>
  )
}
