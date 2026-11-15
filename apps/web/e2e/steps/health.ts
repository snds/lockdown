// The vault lane: Health (the Security screen).

import type { Locator, Page } from "@playwright/test"

/** The quantum-safe row of Health's Protections (brief H1): the core's headline, its explanation and the devices it waits for. */
export const quantumLine = (page: Page): Locator => page.getByRole("status", { name: "Quantum-safe signatures" })

/** The devices the line names, each a way to that device: all of them, or one by name. */
export const lineDevices = (line: Locator, name?: string): Locator => (name ? line.getByRole("button", { name }) : line.getByRole("button"))
