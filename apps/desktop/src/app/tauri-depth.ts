import { invoke } from "@tauri-apps/api/core"

import type { DepthClient } from "@/features/depth/depth-client"
import { splitOncePage } from "@/core/secret-bytes"
import type { GateInput } from "@/core/types"

const g = (gate?: GateInput) => (gate ? { gate } : {})

/**
 * The desktop's security-depth client: one Tauri command per method
 * (apps/desktop/src-tauri/src/depth.rs). Only `App.tsx` imports this;
 * feature code reaches it through `DepthClientProvider`.
 */
export const tauriDepthClient: DepthClient = {
  exposure: (id) => invoke("exposure", { id }),
  exposures: () => invoke("exposures"),
  setExposure: (id, tier, gate) => invoke("set_exposure", { id, tier, ...g(gate) }),
  legacyOptIn: (id) => invoke("legacy_opt_in", { id }),
  setLegacyOptIn: (id, on, gate) => invoke("set_legacy_opt_in", { id, on, ...g(gate) }),
  reminders: () => invoke("reminders"),
  passwordCheck: (password) => invoke("password_check", { password }),
  passwordSnooze: () => invoke("password_snooze"),
  setPasswordReminderDays: (days) => invoke("set_password_reminder_days", { days }),
  drillSnooze: () => invoke("drill_snooze"),
  kitTestSkip: (understood) => invoke("kit_test_skip", { understood }),
  drillChecks: () => invoke("drill_checks"),
  phraseInvite: (login) => invoke("phrase_invite", { login }),
  phraseIncoming: () => invoke("phrase_incoming"),
  phraseAnswer: (verifyId, phrase) => invoke("phrase_answer", { verifyId, phrase }),
  phrasePoll: (verifyId) => invoke("phrase_poll", { verifyId }),
  phraseCancel: (verifyId) => invoke("phrase_cancel", { verifyId }),
  securityKeys: () => invoke("security_keys"),
  securityKeyAdd: () => invoke("security_key_add"),
  securityKeyRemove: (id, gate) => invoke("security_key_remove", { id, ...g(gate) }),
  linkOffer: () => invoke("link_offer"),
  linkPoll: () => invoke("link_poll"),
  linkApprove: (gate) => invoke("link_approve", g(gate)),
  linkDecline: () => invoke("link_decline"),
  secretKeyStatus: () => invoke("secret_key_status"),
  joinStart: (server, invite) => invoke("join_start", { server, invite }),
  joinPoll: () => invoke("join_poll"),
  joinFinish: (login, password) => invoke("join_finish", { login, password }),
  joinCancel: () => invoke("join_cancel"),
  secondFactorOpen: (server) => invoke("second_factor_open", { server }),
  secondFactorWait: () => invoke("second_factor_wait"),
  accountRegisterOptedOut: async (input, understood) =>
    splitOncePage(new Uint8Array(await invoke<ArrayBuffer>("account_register_opted_out", { ...input, understood }))),
}
