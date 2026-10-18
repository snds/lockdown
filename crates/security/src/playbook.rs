//! What to do about each kind of finding, in a few plain steps. Every
//! shell shows the same words, so they live here and not in a UI. Each
//! playbook ends with how to confirm the fix worked.

use serde::Serialize;

/// One playbook.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Playbook {
    /// Its id, which a finding names.
    pub id: &'static str,
    /// What happened, as a heading.
    pub title: &'static str,
    /// Why it matters, in one or two sentences.
    pub summary: &'static str,
    /// What to do, in order; the last step confirms the fix.
    pub steps: &'static [&'static str],
}

/// Every playbook.
pub const PLAYBOOKS: [Playbook; 7] = [
    Playbook {
        id: "breached-password",
        title: "This password has shown up in a data breach",
        summary: "Criminals share lists of passwords stolen from websites. This exact password is on one of those lists, so anyone can try it.",
        steps: &[
            "Change this password now. Use Change password to go to the site, and let Lockdown make the new one.",
            "If you used the same password anywhere else, change it there too. Lockdown lists those places as reused passwords.",
            "Turn on two-step sign-in for this site if it offers it.",
            "Look at the site's security or recent-activity page for sign-ins you don't recognise, and sign out other sessions if it lets you.",
            "Come back and choose Check now. The warning goes away once the new password is saved.",
        ],
    },
    Playbook {
        id: "reused-password",
        title: "The same password is used in more than one place",
        summary: "If any one of these sites is breached, whoever gets the password can try it on all the others.",
        steps: &[
            "Start with the most important account: your email, your bank, anything that holds money or can reset other passwords.",
            "Change its password to a new one Lockdown makes. Every site should have its own.",
            "Do the same for the other places in the list.",
            "Choose Check now. The warning clears when no two items share a password.",
        ],
    },
    Playbook {
        id: "weak-password",
        title: "This password is easy to guess",
        summary: "It is short, common, or made of words and patterns that guessing tools try first.",
        steps: &[
            "Change it to a new password Lockdown makes for you.",
            "You don't need to remember it: Lockdown fills it in or copies it for you.",
            "Choose Check now to confirm it is no longer marked.",
        ],
    },
    Playbook {
        id: "missing-2fa",
        title: "This site offers two-step sign-in",
        summary: "With two-step sign-in a stolen password isn't enough: the site also asks for a code or a key that only you have.",
        steps: &[
            "Open the site's set-up page with Turn it on, and follow its steps.",
            "If the site offers an authenticator app code, choose that one: Lockdown can keep the code with this login. Edit the item and add the set-up key the site shows you.",
            "Save the backup codes the site gives you, in a Note in Lockdown.",
            "If you already use two-step sign-in here (by text message, for example), dismiss this. Choose Check now once a code is saved to confirm.",
        ],
    },
    Playbook {
        id: "lookalike-site",
        title: "This website's address looks like another site's",
        summary: "Its address is very close to a well-known site's or to one of your other logins. Fake sites copy real addresses to collect passwords.",
        steps: &[
            "Compare the two addresses letter by letter.",
            "If you didn't mean to save this site, don't sign in there. Move this item to the trash.",
            "If you typed a real password into it, change that password on the real site now and turn on two-step sign-in there.",
            "If both sites are genuine and yours, dismiss this warning.",
        ],
    },
    Playbook {
        id: "exposed-email",
        title: "Your email address was in a data breach",
        summary: "A website you have, or had, an account with lost its list of customers, and your email address was on it.",
        steps: &[
            "Change your password on the site that was breached. If you no longer use it, close the account if you can.",
            "If the breach included passwords, change that password anywhere else you used it.",
            "Be wary of emails about the breach: criminals use them to trick people. Go to sites directly, not through links in emails.",
            "Turn on two-step sign-in for your email account; it is the key that can reset every other password.",
            "Dismiss this once you have done the steps that apply.",
        ],
    },
    Playbook {
        id: "new-sign-in",
        title: "A new device signed in to your Lockdown account",
        summary: "A device this one hasn't seen before signed in with your account. If it was you, there is nothing to do.",
        steps: &[
            "If it was you (a new computer or phone), dismiss this.",
            "If it wasn't you, go to Devices and revoke that device now.",
            "Then change your Lockdown account password and make a new recovery kit.",
            "Look at Devices again: only your own devices should be listed.",
        ],
    },
];

/// The playbook with `id`.
pub fn playbook(id: &str) -> Option<&'static Playbook> {
    PLAYBOOKS.iter().find(|p| p.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_playbook_is_short_plain_and_ends_with_a_check() {
        let mut ids: Vec<&str> = PLAYBOOKS.iter().map(|p| p.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), PLAYBOOKS.len(), "ids are unique");
        for p in PLAYBOOKS {
            assert!((3..=6).contains(&p.steps.len()), "{}", p.id);
            for text in std::iter::once(p.title)
                .chain([p.summary])
                .chain(p.steps.iter().copied())
            {
                // No security jargon a person would have to look up.
                for word in [
                    "k-anonymity",
                    "SHA",
                    "hash",
                    "TOTP",
                    "2FA",
                    "MFA",
                    "phishing",
                    "entropy",
                ] {
                    assert!(!text.contains(word), "{}: {word} in {text}", p.id);
                }
            }
            let last = p.steps.last().unwrap();
            assert!(
                ["Check now", "dismiss", "Dismiss", "Look at", "Devices"]
                    .iter()
                    .any(|w| last.contains(w)),
                "{} ends without a way to confirm: {last}",
                p.id
            );
        }
        assert!(playbook("breached-password").is_some());
        assert!(playbook("nope").is_none());
    }
}
