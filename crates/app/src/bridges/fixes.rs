//! Per-site fill fixes (docs/plan/23-bridges.md §5): which field on a
//! site's page is which, or "never fill here", when the page's own markup
//! misleads the extension.
//!
//! **The person's own** live on each login for the site, in the reserved
//! field `lockdown.fill`, so they sync end to end, travel with the item's
//! history and export, and are never shown as a field. Setting one from
//! the extension's popup sets it on every everyday login saved for that
//! site (what the person sees as "this site").
//!
//! **The community's** come from the signed site data's `fixes`
//! (the matcher's signed-data format), read only, under the person's own:
//! a field the person fixed keeps their role; "never" from either wins.
//!
//! A field is named by its `id`, `name` or `autocomplete` attribute,
//! exactly, never by a selector (a page is hostile and a selector is
//! code); a fix has at most [`MAX_FIELDS`] fields and each name at most
//! [`MAX_NAME`] characters.

use lockdown_matcher::Origin;
use lockdown_security::site_data::{FIELD_ROLES, FieldFix, FieldMatch, SiteData};
use serde::{Deserialize, Serialize};

use crate::session::bridges::{FILL_FIELD, LoginFacts};
use crate::{App, AppError, AppResult};

/// The most fields one fix names.
pub const MAX_FIELDS: usize = 20;
/// The longest attribute value a fix may match.
pub const MAX_NAME: usize = 128;

/// A fix as the person sets it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FillFix {
    /// Never offer a fill on this site.
    #[serde(default)]
    pub never: bool,
    /// Fields whose role the page gets wrong.
    #[serde(default)]
    pub fields: Vec<FieldFix>,
}

/// One field's fix as the extension applies it, with where it came from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AppliedField {
    #[serde(rename = "match")]
    pub matcher: FieldMatch,
    pub role: String,
    /// `yours` or `community`.
    pub source: &'static str,
}

/// What applies on one page.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Default)]
pub struct SiteFixView {
    /// The page's host.
    pub host: String,
    /// Never fill here.
    pub never: bool,
    /// Who said never: `yours` or `community`.
    pub never_source: Option<&'static str>,
    pub fields: Vec<AppliedField>,
    /// The community fix's note, when one applies.
    pub note: Option<String>,
    /// How many of the person's logins carry their own fix for the site.
    pub yours: usize,
}

fn clean_attr(v: &Option<String>) -> Result<Option<String>, String> {
    match v.as_deref().map(str::trim) {
        None | Some("") => Ok(None),
        Some(t)
            if t.chars().count() <= MAX_NAME
                && t.chars().all(|c| !c.is_control() && c != '"' && c != '\\') =>
        {
            Ok(Some(t.to_string()))
        }
        Some(_) => Err(format!(
            "a field name in a fix must be at most {MAX_NAME} plain characters"
        )),
    }
}

/// Checks and tidies a fix from a shell or the extension.
pub fn check_fix(fix: &FillFix) -> Result<FillFix, String> {
    if fix.fields.len() > MAX_FIELDS {
        return Err(format!("a fix names at most {MAX_FIELDS} fields"));
    }
    let mut fields = Vec::new();
    for f in &fix.fields {
        if !FIELD_ROLES.contains(&f.role.as_str()) {
            return Err(format!("'{}' isn't something a field can be", f.role));
        }
        let matcher = FieldMatch {
            id: clean_attr(&f.matcher.id)?,
            name: clean_attr(&f.matcher.name)?,
            autocomplete: clean_attr(&f.matcher.autocomplete)?,
        };
        if matcher.id.is_none() && matcher.name.is_none() && matcher.autocomplete.is_none() {
            return Err("each field in a fix needs its id, name or autocomplete".into());
        }
        if !fields.iter().any(|x: &FieldFix| x.matcher == matcher) {
            fields.push(FieldFix {
                matcher,
                role: f.role.clone(),
            });
        }
    }
    Ok(FillFix {
        never: fix.never,
        fields,
    })
}

fn host_key(o: &Origin) -> String {
    o.registrable_domain().unwrap_or_else(|| o.host.clone())
}

/// The everyday logins saved for the page's site.
fn logins_for<'a>(logins: &'a [LoginFacts], page: &Origin) -> Vec<&'a LoginFacts> {
    let key = host_key(page);
    logins
        .iter()
        .filter(|f| !f.crown_jewel)
        .filter(|f| {
            f.urls
                .iter()
                .filter_map(|u| Origin::parse(u).ok())
                .any(|o| o.host == page.host || host_key(&o) == key)
        })
        .collect()
}

/// What applies on `page` given the person's logins and the signed data.
fn view_for(page: &Origin, logins: &[LoginFacts], data: &SiteData) -> SiteFixView {
    let mut view = SiteFixView {
        host: page.host.clone(),
        ..SiteFixView::default()
    };
    for f in logins_for(logins, page) {
        let Some(fix) = f
            .fill
            .as_deref()
            .and_then(|t| serde_json::from_str::<FillFix>(t).ok())
        else {
            continue;
        };
        view.yours += 1;
        if fix.never {
            view.never = true;
            view.never_source = Some("yours");
        }
        for field in fix.fields {
            if !view.fields.iter().any(|x| x.matcher == field.matcher) {
                view.fields.push(AppliedField {
                    matcher: field.matcher,
                    role: field.role,
                    source: "yours",
                });
            }
        }
    }
    if let Some(community) = data.fix(&page.host) {
        if community.never && !view.never {
            view.never = true;
            view.never_source = Some("community");
        }
        for field in &community.fields {
            if !view.fields.iter().any(|x| x.matcher == field.matcher) {
                view.fields.push(AppliedField {
                    matcher: field.matcher.clone(),
                    role: field.role.clone(),
                    source: "community",
                });
            }
        }
        if !community.note.is_empty() {
            view.note = Some(community.note.clone());
        }
    }
    view
}

fn page_origin(frame: &str) -> AppResult<Origin> {
    let o = Origin::parse(frame).map_err(|_| AppError::refused("the page isn't a web address"))?;
    if o.scheme != "https" && o.scheme != "http" {
        return Err(AppError::refused("fixes are for web pages"));
    }
    Ok(o)
}

impl App {
    /// The fix that applies on `frame` (a page address from the browser).
    pub fn site_fix_for(&self, frame: &str) -> AppResult<SiteFixView> {
        let page = page_origin(frame)?;
        self.call(move |core| {
            let logins = core.session()?.bridge_logins().map_err(AppError::refused)?;
            Ok(view_for(&page, &logins, SiteData::builtin()))
        })
    }

    /// A login's own fix, for the editor.
    pub fn site_fix(&self, item: &str) -> AppResult<FillFix> {
        let item =
            lockdown_vault::Id::parse(item).map_err(|_| AppError::refused("that isn't an item"))?;
        self.call(move |core| {
            let logins = core.session()?.bridge_logins().map_err(AppError::refused)?;
            let f = logins
                .iter()
                .find(|f| f.item_id == item)
                .ok_or_else(|| AppError::refused("that isn't a live login"))?;
            Ok(f.fill
                .as_deref()
                .and_then(|t| serde_json::from_str(t).ok())
                .unwrap_or_default())
        })
    }

    /// Sets (or, when empty, clears) one login's fix. A protected item is
    /// never filled, so it takes none.
    pub fn site_fix_set(&self, item: &str, fix: FillFix) -> AppResult<FillFix> {
        let item =
            lockdown_vault::Id::parse(item).map_err(|_| AppError::refused("that isn't an item"))?;
        let fix = check_fix(&fix).map_err(AppError::refused)?;
        self.call(move |core| {
            let logins = core.session()?.bridge_logins().map_err(AppError::refused)?;
            let f = logins
                .iter()
                .find(|f| f.item_id == item)
                .ok_or_else(|| AppError::refused("that isn't a live login"))?;
            if f.crown_jewel {
                return Err(AppError::refused(
                    "protected items are never filled, so they need no fix",
                ));
            }
            let value = (fix.never || !fix.fields.is_empty())
                .then(|| serde_json::to_string(&fix))
                .transpose()
                .map_err(|e| AppError::refused(e.to_string()))?;
            core.session()?
                .bridge_set(item, FILL_FIELD, value, &[])
                .map_err(AppError::refused)?;
            Ok(fix.clone())
        })
    }

    /// Sets the fix on every everyday login saved for `frame`'s site (the
    /// extension's "fix this page"). Returns how many logins took it.
    pub fn site_fix_set_for(&self, frame: &str, fix: FillFix) -> AppResult<usize> {
        let page = page_origin(frame)?;
        let fix = check_fix(&fix).map_err(AppError::refused)?;
        self.call(move |core| {
            let logins = core.session()?.bridge_logins().map_err(AppError::refused)?;
            let targets: Vec<_> = logins_for(&logins, &page)
                .into_iter()
                .filter(|f| f.editable)
                .map(|f| f.item_id)
                .collect();
            if targets.is_empty() {
                return Err(AppError::refused(
                    "there's no login saved for this site to keep the fix on; save one first",
                ));
            }
            let value = (fix.never || !fix.fields.is_empty())
                .then(|| serde_json::to_string(&fix))
                .transpose()
                .map_err(|e| AppError::refused(e.to_string()))?;
            for id in &targets {
                core.session()?
                    .bridge_set(*id, FILL_FIELD, value.clone(), &[])
                    .map_err(AppError::refused)?;
            }
            Ok(targets.len())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(name: &str, role: &str) -> FieldFix {
        FieldFix {
            matcher: FieldMatch {
                name: Some(name.into()),
                ..FieldMatch::default()
            },
            role: role.into(),
        }
    }

    #[test]
    fn a_fix_is_checked_before_it_is_kept() {
        let ok = check_fix(&FillFix {
            never: false,
            fields: vec![
                field(" login_id ", "username"),
                field("login_id", "username"),
            ],
        })
        .unwrap();
        assert_eq!(ok.fields.len(), 1, "duplicates fold");
        assert_eq!(ok.fields[0].matcher.name.as_deref(), Some("login_id"));
        for bad in [
            FillFix {
                never: false,
                fields: vec![field("x", "click")],
            },
            FillFix {
                never: false,
                fields: vec![FieldFix {
                    matcher: FieldMatch::default(),
                    role: "username".into(),
                }],
            },
            FillFix {
                never: false,
                fields: vec![field(&"x".repeat(129), "username")],
            },
            FillFix {
                never: false,
                fields: vec![field("a\"]; body", "username")],
            },
            FillFix {
                never: false,
                fields: (0..21).map(|i| field(&format!("f{i}"), "ignore")).collect(),
            },
        ] {
            assert!(check_fix(&bad).is_err(), "{bad:?}");
        }
    }
}
