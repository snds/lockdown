import { render, screen, within } from "@testing-library/react"

import { ItemGroup } from "@unicron/ui"

import { malformedLists } from "@/test/a11y"

import { ListItems } from "./list-items"

describe("ListItems (design review R7)", () => {
  it("gives each child of unicron's ItemGroup its own list item, fragments opened and empty children left out", () => {
    const { container } = render(
      <ItemGroup aria-label="Sign-in methods">
        <ListItems>
          <p>Username</p>
          {null}
          {false}
          <>
            <p>Password</p>
            <p>Passkeys</p>
          </>
          {[<p key="a">Code one</p>, <p key="b">Code two</p>]}
        </ListItems>
      </ItemGroup>,
    )
    const list = screen.getByRole("list", { name: "Sign-in methods" })
    const items = within(list).getAllByRole("listitem")
    expect(items.map((i) => i.textContent)).toEqual(["Username", "Password", "Passkeys", "Code one", "Code two"])
    expect(malformedLists(container)).toEqual([])
  })

  it("is what the check catches without it: an ItemGroup's plain children", () => {
    const { container } = render(
      <ItemGroup aria-label="Websites">
        <p>https://example.test</p>
      </ItemGroup>,
    )
    expect(malformedLists(container)).toEqual(["Websites: p"])
  })
})
