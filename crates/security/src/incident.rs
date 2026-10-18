//! The seven guided incident flows (docs/plan/23-bridges.md §6;
//! feature matrix `breach-playbooks`): what to do, in order, when
//! something bigger than one finding has happened — a site you use was
//! breached, someone is trying your password elsewhere, your
//! authenticator secrets leaked, your mailbox or a sign-in provider was
//! taken over, a device has malware, or your vault itself is at risk.
//!
//! The per-finding playbooks ([`crate::playbook`]) say how to fix one
//! thing. A flow is longer and ordered: it starts with what stops the
//! damage, reaches the accounts that hang off the one that fell, covers
//! sessions, forwarding rules, recovery details and app grants, and ends
//! with a step that checks it worked. `lockdown-app` fills each step's
//! list of affected items from the vault; the words live here so every
//! shell says the same.

use serde::Serialize;

/// What a flow asks the person to pick before it starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    /// Nothing: it is about everything.
    None,
    /// One login (the breached site).
    Item,
    /// One email address.
    Email,
    /// One sign-in provider.
    Provider,
}

/// What a step can do in the app, beside its words.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// Words only.
    Read,
    /// Change the password of each listed item (Change password per item).
    ChangePasswords,
    /// Turn on two-step sign-in for the listed items (their set-up cards).
    TwoStep,
    /// Set up each listed code again with a new key.
    ReenrolCodes,
    /// End Lockdown's other sessions (Devices → Signed in).
    Sessions,
    /// Remove Lockdown devices you don't trust (Devices).
    Devices,
    /// Make a new recovery kit (Recovery).
    RecoveryKit,
    /// Run the security center's checks again.
    Check,
}

/// Which items a step lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Affects {
    /// None.
    Nothing,
    /// The login the person picked.
    Picked,
    /// Logins sharing the picked login's password.
    SamePassword,
    /// Every login whose password is reused or seen in a breach.
    ReusedOrBreached,
    /// Every item holding a one-time code.
    Codes,
    /// The mailbox's own login for the picked address.
    Mailbox,
    /// Logins whose user name is the picked address.
    UsingEmail,
    /// The picked provider's own login.
    ProviderAccount,
    /// Logins that sign in through the picked provider.
    SignsInWithProvider,
    /// Crown jewels, mailboxes and sign-in providers: what to change first.
    MostImportant,
    /// Every other live login.
    EveryLogin,
}

/// One step.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Step {
    /// Its id within the flow.
    pub id: &'static str,
    /// What to do, as a short line.
    pub title: &'static str,
    /// How, in a sentence or two.
    pub detail: &'static str,
    /// What the app can do for it.
    pub action: Action,
    /// Which items it lists.
    pub affects: Affects,
}

/// One flow.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Incident {
    /// Its id.
    pub id: &'static str,
    /// What happened, as a heading.
    pub title: &'static str,
    /// When to use it, in a sentence.
    pub when: &'static str,
    /// What the person picks first.
    pub scope: Scope,
    /// The steps, in order; the last one checks it worked.
    pub steps: &'static [Step],
}

const fn step(
    id: &'static str,
    title: &'static str,
    detail: &'static str,
    action: Action,
    affects: Affects,
) -> Step {
    Step {
        id,
        title,
        detail,
        action,
        affects,
    }
}

/// Every flow, in the order the security center lists them.
pub const INCIDENTS: [Incident; 7] = [
    Incident {
        id: "site-breach",
        title: "A site I use was breached",
        when: "The site told you, or the news did, that its accounts or passwords were stolen.",
        scope: Scope::Item,
        steps: &[
            step(
                "change",
                "Change the password on that site",
                "Use Change password and let Lockdown make the new one. Do this even if the site says passwords were protected.",
                Action::ChangePasswords,
                Affects::Picked,
            ),
            step(
                "same",
                "Change it wherever else you used it",
                "Anyone with the stolen list will try the same password on other sites. These logins use the same one.",
                Action::ChangePasswords,
                Affects::SamePassword,
            ),
            step(
                "two-step",
                "Turn on two-step sign-in there",
                "A stolen password is then not enough on its own.",
                Action::TwoStep,
                Affects::Picked,
            ),
            step(
                "sessions",
                "Sign out everywhere on that site",
                "In the site's security settings, look for sessions, devices or \"sign out everywhere\", and end the ones you don't recognise.",
                Action::Read,
                Affects::Picked,
            ),
            step(
                "recovery",
                "Check its recovery details",
                "Make sure the recovery email and phone number on the account are still yours, so nobody can reset it.",
                Action::Read,
                Affects::Picked,
            ),
            step(
                "verify",
                "Check it worked",
                "Sign in with the new password, then choose Check now in Lockdown: nothing for this site should be left in Fix now.",
                Action::Check,
                Affects::Nothing,
            ),
        ],
    },
    Incident {
        id: "credential-stuffing",
        title: "Someone is trying my password on other sites",
        when: "A site warned you about sign-in attempts you didn't make, or locked your account after failed tries.",
        scope: Scope::None,
        steps: &[
            step(
                "most",
                "Change the passwords that are reused or breached",
                "These are the ones such attempts work against. Start with your email and anything that holds money.",
                Action::ChangePasswords,
                Affects::ReusedOrBreached,
            ),
            step(
                "two-step",
                "Turn on two-step sign-in where it's offered",
                "It stops a correct password from being enough.",
                Action::TwoStep,
                Affects::ReusedOrBreached,
            ),
            step(
                "activity",
                "Look at each site's recent activity",
                "Where a site shows recent sign-ins, end any you don't recognise.",
                Action::Read,
                Affects::Nothing,
            ),
            step(
                "verify",
                "Check it worked",
                "Choose Check now: no password should be listed as reused or breached.",
                Action::Check,
                Affects::Nothing,
            ),
        ],
    },
    Incident {
        id: "seed-leak",
        title: "My authenticator codes' set-up keys leaked",
        when: "A backup or export of your authenticator, or a screenshot of a QR code, got out.",
        scope: Scope::None,
        steps: &[
            step(
                "reenrol",
                "Set up two-step sign-in again on each site",
                "On each site, turn two-step sign-in off and on again to get a new set-up key, and save the new key in Lockdown. The old codes stop working.",
                Action::ReenrolCodes,
                Affects::Codes,
            ),
            step(
                "backup-codes",
                "Make new backup codes",
                "Sites that gave you backup codes let you make new ones; the old ones may be in the same leak.",
                Action::Read,
                Affects::Codes,
            ),
            step(
                "files",
                "Delete the leaked file",
                "Remove the exported file or screenshot from wherever it was saved, synced or sent.",
                Action::Read,
                Affects::Nothing,
            ),
            step(
                "verify",
                "Check it worked",
                "Sign in to one of the sites with the code Lockdown shows now; it should be accepted, and an old code from the leak should not.",
                Action::Read,
                Affects::Nothing,
            ),
        ],
    },
    Incident {
        id: "email-takeover",
        title: "Someone got into my email",
        when: "You see mail you didn't send, a password-reset you didn't ask for, or you can't get into your mailbox.",
        scope: Scope::Email,
        steps: &[
            step(
                "recover",
                "Get back into the mailbox",
                "If you're locked out, use the provider's account recovery now; it gets harder the longer someone else is in.",
                Action::Read,
                Affects::Mailbox,
            ),
            step(
                "password",
                "Change the mailbox password",
                "Let Lockdown make a new one, and turn on two-step sign-in if it isn't on.",
                Action::ChangePasswords,
                Affects::Mailbox,
            ),
            step(
                "rules",
                "Remove forwarding rules and filters you didn't make",
                "Attackers add rules that forward or hide reset emails. Check forwarding, filters and any \"send mail as\" addresses.",
                Action::Read,
                Affects::Mailbox,
            ),
            step(
                "recovery",
                "Check the recovery email and phone",
                "Make sure they are yours and remove any you don't know.",
                Action::Read,
                Affects::Mailbox,
            ),
            step(
                "apps",
                "Remove apps and sessions you don't know",
                "In the mailbox's security settings, end other sessions and remove connected apps you don't recognise.",
                Action::Read,
                Affects::Mailbox,
            ),
            step(
                "accounts",
                "Change the passwords of accounts that use this address",
                "Whoever had your mailbox could reset these. Change each, starting with the ones that hold money.",
                Action::ChangePasswords,
                Affects::UsingEmail,
            ),
            step(
                "verify",
                "Check it worked",
                "Send yourself a test message, check no new rule appears, and choose Check now in Lockdown.",
                Action::Check,
                Affects::Nothing,
            ),
        ],
    },
    Incident {
        id: "provider-takeover",
        title: "Someone got into an account I sign in with",
        when: "Your Google, Apple, Microsoft or other account that other sites let you sign in with was taken over.",
        scope: Scope::Provider,
        steps: &[
            step(
                "secure",
                "Secure that account first",
                "Recover it if you're locked out, change its password and turn on two-step sign-in.",
                Action::ChangePasswords,
                Affects::ProviderAccount,
            ),
            step(
                "grants",
                "Remove apps and sites you didn't connect",
                "In the account's security settings, review third-party access and connected apps, and remove what you don't recognise.",
                Action::Read,
                Affects::ProviderAccount,
            ),
            step(
                "sessions",
                "End its other sessions and devices",
                "Sign out every device you don't recognise from the account.",
                Action::Read,
                Affects::ProviderAccount,
            ),
            step(
                "sites",
                "Check every site you sign in to with it",
                "Each of these let whoever had the account in. Sign in to each, check its activity, and add a password or passkey of its own where it lets you.",
                Action::Read,
                Affects::SignsInWithProvider,
            ),
            step(
                "verify",
                "Check it worked",
                "Sign in to the account and one of the sites: no unknown session or app should be left.",
                Action::Read,
                Affects::Nothing,
            ),
        ],
    },
    Incident {
        id: "infostealer",
        title: "A computer of mine has malware",
        when: "Your antivirus found something, or you ran a program you shouldn't have, on a computer where you sign in to things.",
        scope: Scope::None,
        steps: &[
            step(
                "clean",
                "Clean or reset that computer first",
                "Until it is clean, anything you type there can be stolen again. Do the next steps from another device.",
                Action::Read,
                Affects::Nothing,
            ),
            step(
                "lockdown-sessions",
                "End Lockdown's other sessions",
                "This signs out every other place your Lockdown account is in use.",
                Action::Sessions,
                Affects::Nothing,
            ),
            step(
                "lockdown-devices",
                "Remove that computer from Lockdown",
                "Remove it in Devices; add it back once it is clean.",
                Action::Devices,
                Affects::Nothing,
            ),
            step(
                "important",
                "Change your most important passwords",
                "Malware takes saved passwords and signed-in sessions. Start with these: your email, the accounts you sign in with, and your protected items.",
                Action::ChangePasswords,
                Affects::MostImportant,
            ),
            step(
                "everything",
                "Change the rest",
                "Then every other login, and sign out other sessions on each site where it lets you.",
                Action::ChangePasswords,
                Affects::EveryLogin,
            ),
            step(
                "verify",
                "Check it worked",
                "Choose Check now, and look at Devices and Signed in: only the devices you trust should be there.",
                Action::Check,
                Affects::Nothing,
            ),
        ],
    },
    Incident {
        id: "vault-breach",
        title: "My Lockdown account or recovery kit may be exposed",
        when: "Your account password or your recovery kit was seen by someone else, or a device with Lockdown on it was stolen.",
        scope: Scope::None,
        steps: &[
            step(
                "sessions",
                "End every other session",
                "This stops anyone else's copy of your sign-in from syncing.",
                Action::Sessions,
                Affects::Nothing,
            ),
            step(
                "devices",
                "Remove devices you don't trust",
                "A removed device can't write to your vaults again.",
                Action::Devices,
                Affects::Nothing,
            ),
            step(
                "kit",
                "Make a new recovery kit",
                "The old kit stops working. Print or save the new one and put it somewhere safe.",
                Action::RecoveryKit,
                Affects::Nothing,
            ),
            step(
                "important",
                "Change your most important passwords",
                "Start with your email, the accounts you sign in with and your protected items.",
                Action::ChangePasswords,
                Affects::MostImportant,
            ),
            step(
                "everything",
                "Change the rest over the next days",
                "Work through the rest of your logins.",
                Action::ChangePasswords,
                Affects::EveryLogin,
            ),
            step(
                "verify",
                "Check it worked",
                "Choose Check now; in Devices only your own devices should be listed, and the account's history should show nothing you didn't do.",
                Action::Check,
                Affects::Nothing,
            ),
        ],
    },
];

/// The flow with `id`.
pub fn incident(id: &str) -> Option<&'static Incident> {
    INCIDENTS.iter().find(|i| i.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seven_flows_each_ending_in_a_check_in_plain_words() {
        let ids: Vec<&str> = INCIDENTS.iter().map(|i| i.id).collect();
        assert_eq!(
            ids,
            [
                "site-breach",
                "credential-stuffing",
                "seed-leak",
                "email-takeover",
                "provider-takeover",
                "infostealer",
                "vault-breach"
            ]
        );
        for i in INCIDENTS {
            assert!(i.steps.len() >= 4, "{}", i.id);
            let last = i.steps.last().unwrap();
            assert_eq!(last.id, "verify", "{}", i.id);
            assert!(last.title.starts_with("Check it worked"));
            let mut seen = std::collections::BTreeSet::new();
            for s in i.steps {
                assert!(seen.insert(s.id), "{}: {} twice", i.id, s.id);
                for jargon in ["TOTP", "2FA", "MFA", "OAuth", "SSO", "token", "credential"] {
                    assert!(
                        !s.detail.contains(jargon) && !s.title.contains(jargon),
                        "{}: {jargon}",
                        i.id
                    );
                }
            }
            assert!(incident(i.id).is_some());
        }
        // The flows reach what the plan names: sessions, forwarding rules,
        // recovery info and app grants.
        let all: String = INCIDENTS
            .iter()
            .flat_map(|i| i.steps.iter())
            .map(|s| format!("{} {}", s.title, s.detail))
            .collect();
        for needed in ["session", "forwarding", "recovery", "connected apps"] {
            assert!(all.contains(needed), "{needed}");
        }
    }
}
