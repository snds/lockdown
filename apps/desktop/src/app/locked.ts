import { createContext, useContext } from "react"

/**
 * Whether the core says the vault is locked, as the components that hold
 * a revealed value read it: unicron's SecretField and LargeTypeView mask
 * at once on `locked`. The screens behind the lock unmount anyway (the
 * unlock screen replaces them); this is the belt to that braces.
 */
const LockedContext = createContext(false)

export const LockedProvider = LockedContext.Provider

export function useLocked(): boolean {
  return useContext(LockedContext)
}
