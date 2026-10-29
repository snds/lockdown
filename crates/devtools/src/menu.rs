//! The Developer menu's items, in the order shown (docs/plan/40-devtools.md
//! §4): each with its id, its words and the action the main window is sent.
//! The front end keeps the same list for the review harness's drawing of
//! the menu (`apps/desktop/src/devtools/menu.ts`); a test keeps them equal.

/// One item: (id, words, action).
pub type Item = (&'static str, &'static str, &'static str);

/// The demo's items.
pub const DEMO: [Item; 3] = [
    ("devtools.demo.open", "Open the demo", "demo_open"),
    ("devtools.demo.reset", "Reset demo data", "demo_reset"),
    ("devtools.demo.load", "Load a demo pack…", "demo_load"),
];

/// The reset's item, last, after a separator.
pub const RESET: Item = ("devtools.reset", "Reset Lockdown…", "reset");

/// The items this build has.
pub fn items() -> Vec<Item> {
    let mut out = Vec::new();
    if cfg!(feature = "demo") {
        out.extend(DEMO);
    }
    if cfg!(feature = "reset") {
        out.push(RESET);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_front_ends_copy_of_the_menu_matches() {
        let ts = include_str!("../../../apps/desktop/src/devtools/menu.ts");
        for (_, words, action) in DEMO.iter().chain(std::iter::once(&RESET)) {
            let line = format!("{{ label: \"{words}\", action: \"{action}\" }}");
            assert!(ts.contains(&line), "menu.ts lacks {line}");
        }
    }
}
