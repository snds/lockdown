import type { DepthClient } from "@/features/depth/depth-client"
import type { GateInput } from "@/core/types"

import type { Link } from "./link"

function notHere(what: string): Promise<never> {
  return Promise.reject({ code: "refused", message: `${what} isn't in the web vault; use the desktop app for it` })
}

/**
 * The desktop's security-depth client over the web vault's worker
 * (crates/web/src/commands/depth.rs carries the desktop's command names).
 * Where the web differs: a key is added on the server's page in a new tab
 * (this tab's vault lives in its memory, so it must not navigate away);
 * joining from another device is this tab's own (`web_join_*`, on this
 * page's server only); accounts are never made here (D176).
 */
export function webDepthClient(link: Link, open: (url: string) => void): DepthClient {
  const call = <T,>(cmd: string, args?: Record<string, unknown>) => link.call<T>(cmd, args)
  const g = (gate?: GateInput) => (gate ? { gate } : {})
  return {
    exposure: (id) => call("exposure", { id }),
    exposures: () => call("exposures"),
    setExposure: (id, tier, gate) => call("set_exposure", { id, tier, ...g(gate) }),
    legacyOptIn: (id) => call("legacy_opt_in", { id }),
    setLegacyOptIn: (id, on, gate) => call("set_legacy_opt_in", { id, on, ...g(gate) }),
    reminders: () => call("reminders"),
    passwordCheck: (password) => call("password_check", { password }),
    passwordSnooze: () => call("password_snooze"),
    setPasswordReminderDays: (days) => call("set_password_reminder_days", { days }),
    drillSnooze: () => call("drill_snooze"),
    kitTestSkip: (understood) => call("kit_test_skip", { understood }),
    drillChecks: () => call("drill_checks"),
    phraseInvite: (login) => call("phrase_invite", { login }),
    phraseIncoming: () => call("phrase_incoming"),
    phraseAnswer: (verifyId, phrase) => call("phrase_answer", { verifyId, phrase }),
    phrasePoll: (verifyId) => call("phrase_poll", { verifyId }),
    phraseCancel: (verifyId) => call("phrase_cancel", { verifyId }),
    securityKeys: () => call("security_keys"),
    securityKeyAdd: async () => open(await call<string>("security_key_add_page")),
    securityKeyRemove: (id, gate) => call("security_key_remove", { id, ...g(gate) }),
    linkOffer: () => call("link_offer"),
    linkPoll: () => call("link_poll"),
    linkApprove: (gate) => call("link_approve", g(gate)),
    linkDecline: () => call("link_decline"),
    secretKeyStatus: () => call("secret_key_status"),
    joinStart: (_server, invite) => call("web_join_start", { invite }),
    joinPoll: () => call("web_join_poll"),
    joinFinish: (login, password) => call("web_join_finish", { login, password }),
    joinCancel: () => call("web_join_cancel"),
    secondFactorOpen: () => notHere("A key page for another device's sign-in"),
    secondFactorWait: () => notHere("Waiting on another device's sign-in"),
    accountRegisterOptedOut: () => notHere("Making an account"),
  }
}
