//! The emergency sheet (docs/plan/23-bridges.md §10): one printable page
//! that says what to do and where things are if this computer is lost,
//! the account password is forgotten, or the server is gone, **with no
//! secret on it**: no password, no Secret Key, no recovery code, no item.
//! It names the account's server and login, this device and the others,
//! where backups go and when the last one ran, and the emergency contacts;
//! it leaves blanks for the person to write where they keep their kit.

use serde::Serialize;

use crate::{App, AppResult, session};

/// One part of the sheet.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SheetSection {
    pub heading: String,
    pub lines: Vec<String>,
}

/// The sheet.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EmergencySheet {
    /// When it was made (seconds since the epoch).
    pub made_at: i64,
    pub title: String,
    pub intro: String,
    pub sections: Vec<SheetSection>,
    /// Lines left blank for the person to fill in by hand.
    pub blanks: Vec<String>,
}

fn when(at: i64) -> String {
    lockdown_cxf::build::date_from_unix(at)
}

impl App {
    /// The emergency sheet, from what this device knows now (the emergency
    /// contacts are asked of the server when there is an account; offline,
    /// the sheet says so).
    pub fn emergency_sheet(&self) -> AppResult<EmergencySheet> {
        let status = self.status()?;
        let devices = self.device_list()?;
        let backups = self.backup_status()?;
        let contacts = match status.account {
            Some(_) => self.emergency().ok(),
            None => None,
        };
        let now = session::now_secs();
        let mut sections = Vec::new();

        let mut account = Vec::new();
        match &status.account {
            Some(a) => {
                account.push(format!("Your Lockdown account is {} on the server {}.", a.login, a.server));
                account.push(
                    "To open it on a new device you need your account password and your recovery kit (the Secret Key and recovery code). Neither is on this page.".into(),
                );
            }
            None => account.push(
                "This vault has no account: it lives on this computer only. Without a backup, losing this computer loses the vault.".into(),
            ),
        }
        account.push(format!(
            "Your recovery kit was last {}.",
            match status.recovery.as_str() {
                "drilled" => "tested and worked",
                "created" => "made, but not tested; test it in Recovery",
                _ => "not made; make one in Recovery",
            }
        ));
        sections.push(SheetSection {
            heading: "Your account".into(),
            lines: account,
        });

        let mut device_lines: Vec<String> = devices
            .iter()
            .filter(|d| d.revoked_at.is_none())
            .map(|d| {
                format!(
                    "{}{} ({}), added {}",
                    d.name,
                    if d.this_device { ", this computer" } else { "" },
                    d.platform,
                    when(d.enrolled_at)
                )
            })
            .collect();
        device_lines.push(
            "If one of these is lost or stolen, remove it in Devices from another one, and end its session in Signed in.".into(),
        );
        sections.push(SheetSection {
            heading: "Your devices".into(),
            lines: device_lines,
        });

        let mut backup_lines = Vec::new();
        if backups.enabled {
            backup_lines.push(format!(
                "Lockdown backs up to {} every {} hours while it is unlocked.",
                backups.folder.as_deref().unwrap_or("a folder you chose"),
                backups.interval_hours
            ));
            backup_lines.push(match backups.last_success_at {
                Some(t) => format!("The last backup that worked was on {}.", when(t)),
                None => "No backup has run yet.".into(),
            });
            backup_lines.push(
                "A backup opens with its backup password in Lockdown's Import, on any computer, with or without an account.".into(),
            );
            if backups.crown_jewels_left_out > 0 {
                backup_lines.push(format!(
                    "{} protected items are left out of backups unless you include them.",
                    backups.crown_jewels_left_out
                ));
            }
        } else {
            backup_lines
                .push("Automatic backups are off. Turn them on in Settings, Backups.".into());
        }
        sections.push(SheetSection {
            heading: "Your backups".into(),
            lines: backup_lines,
        });

        let contact_lines = match contacts {
            None if status.account.is_some() => {
                vec!["Lockdown couldn't reach your server to list your emergency contacts.".into()]
            }
            None => vec!["Emergency contacts need an account.".into()],
            Some(list) => {
                let mine: Vec<String> = list
                    .iter()
                    .filter(|e| e.mine && !matches!(e.state.as_str(), "revoked" | "declined"))
                    .map(|e| format!("{}, after a wait of {} days", e.other, e.wait_days))
                    .collect();
                if mine.is_empty() {
                    vec!["You have no emergency contact. Add one in Sharing so someone you trust can get in if you can't.".into()]
                } else {
                    let mut v = vec![
                        "These people can ask for access; you can stop it during the wait:".into(),
                    ];
                    v.extend(mine);
                    v
                }
            }
        };
        sections.push(SheetSection {
            heading: "Emergency contacts".into(),
            lines: contact_lines,
        });

        sections.push(SheetSection {
            heading: "What to do".into(),
            lines: vec![
                "Lost this computer: from another device remove it in Devices, end its session, then change your account password.".into(),
                "Forgot your account password: use Recover on a device with your recovery kit.".into(),
                "Lost every device: install Lockdown, choose Recover, and type your account password and your recovery kit.".into(),
                "The server is gone: on any computer, open your newest backup in Lockdown's Import with its backup password.".into(),
                "Someone may have your password or kit: open Security and follow \"My Lockdown account or recovery kit may be exposed\".".into(),
            ],
        });

        Ok(EmergencySheet {
            made_at: now,
            title: "Lockdown emergency sheet".into(),
            intro: format!(
                "Made on {} on {}. Keep it with your recovery kit, somewhere safe. It holds no password or key.",
                when(now),
                status.device_name
            ),
            sections,
            blanks: vec![
                "Where I keep my recovery kit:".into(),
                "Where my backups are kept (and copied):".into(),
                "Who to call to help:".into(),
            ],
        })
    }
}
