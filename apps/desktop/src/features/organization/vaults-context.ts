import { createContext, useContext } from "react"

import type { VaultView } from "@/core/types"

interface VaultsValue {
  /** The persona's vaults, the first one first; empty while loading. */
  vaults: VaultView[]
  /** Loads the vaults (and the rows) again after a change. */
  reload: () => void
}

const VaultsContext = createContext<VaultsValue>({ vaults: [], reload: () => {} })

/** The vaults and their folders, loaded with the rows by the app root. */
export const VaultsProvider = VaultsContext.Provider

export function useVaults(): VaultsValue {
  return useContext(VaultsContext)
}

/** A vault's name, for captions; empty when it isn't known. */
export function vaultName(vaults: VaultView[], id: string | undefined): string {
  return vaults.find((v) => v.id === id)?.name ?? ""
}
