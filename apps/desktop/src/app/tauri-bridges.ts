import { invoke } from "@tauri-apps/api/core"

import type { BridgesClient } from "@/features/bridges/bridges-client"

/**
 * The desktop's bridges client: one Tauri command per method
 * (apps/desktop/src-tauri/src/bridges.rs). Only the desktop's `App.tsx`
 * imports this; feature code reaches it through `BridgesProvider`.
 */
export const tauriBridgesClient: BridgesClient = {
  clockStatus: () => invoke("clock_status"),
  clockCheck: () => invoke("clock_check"),
  federationMap: () => invoke("federation_map"),
  federationAccept: (pending, itemId) => invoke("federation_accept", { pending, itemId }),
  federationDismiss: (pending) => invoke("federation_dismiss", { pending }),
  federationSetAccount: (provider, itemId) => invoke("federation_set_account", { provider, itemId }),
  siteFix: (itemId) => invoke("site_fix", { itemId }),
  siteFixSet: (itemId, fix) => invoke("site_fix_set", { itemId, fix }),
  aliasOverview: () => invoke("alias_overview"),
  aliasServiceAdd: (input) => invoke("alias_service_add", { service: input.service, key: input.key, base: input.base ?? null, domain: input.domain ?? null }),
  aliasGenerate: (itemId, site, gate) => invoke("alias_generate", { itemId, site, gate }),
  aliasMonitoring: () => invoke("alias_monitoring"),
  aliasMonitoringSet: (on) => invoke("alias_monitoring_set", { on }),
  incidents: () => invoke("incidents"),
  incident: (id, pick) => invoke("incident", { id, pick: pick ?? null }),
  incidentMark: (id, step, itemId, done) => invoke("incident_mark", { id, step, itemId, done }),
  incidentReset: (id) => invoke("incident_reset", { id }),
  setupGuide: (itemId) => invoke("setup_guide", { itemId }),
  guideSources: () => invoke("guide_sources"),
  emergencySheet: () => invoke("emergency_sheet"),
  exportSaveAge: (input, gate) => invoke("export_save_age", { target: input.target, recipients: input.recipients, vault: input.vault ?? null, gate }),
}
