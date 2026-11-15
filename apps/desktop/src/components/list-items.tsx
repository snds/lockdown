import { Children, Fragment, isValidElement, type ReactElement, type ReactNode } from "react"

/**
 * Each child of a `role="list"` container in its own `role="listitem"`
 * wrapper (design review R7). A detail block holds whatever its kind
 * draws (a secret field, a code, a passkey list, a sentence), none of them a
 * list item. A list whose children aren't items is no list to a screen
 * reader, so every child gets one.
 *
 * The container is a plain `role="list"` element, not unicron's `ItemGroup`:
 * from unicron 1.0 an `Item` inside an `ItemGroup` takes the list-item role
 * itself, which would put an item inside this wrapper's item.
 *
 * Fragments are opened, so `<>{a}{b}</>` is two items, not one; empty
 * children (`null`, `false`) are left out, so a block never announces an
 * empty item. The wrapper is a plain block: in a flex column it takes the
 * child's place, and the child keeps its own layout inside it.
 */
export function ListItems({ children }: { children: ReactNode }) {
  return <>{listItems(children)}</>
}

function listItems(children: ReactNode): ReactElement[] {
  const out: ReactElement[] = []
  const walk = (node: ReactNode, path: string) => {
    for (const child of Children.toArray(node)) {
      const key = isValidElement(child) && child.key !== null ? String(child.key) : String(out.length)
      if (isValidElement<{ children?: ReactNode }>(child) && child.type === Fragment) {
        walk(child.props.children, `${path}${key}/`)
        continue
      }
      out.push(
        <div role="listitem" key={`${path}${key}`}>
          {child}
        </div>,
      )
    }
  }
  walk(children, "")
  return out
}
