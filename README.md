# Lockdown

A cross-platform password manager, passkey provider, and authenticator. The goal is to meet and beat 1Password, Bitwarden, Proton Pass, Vaultwarden, and Apple Passwords.

**Status:** planned (plan set v0.1, 14 decisions). No code yet. The shared identity crate is built first, in Mailbird.

## Plan

Start with [docs/plan/00-big-picture.md](docs/plan/00-big-picture.md). The plan set (00–06 plus `decisions.md`) is the current source of truth; the research below is its evidence.

## Research (15 October 2026)

Start with [the brief](docs/research/2026-10-15/brief.md). Everything else is in [`docs/research/2026-10-15/`](docs/research/2026-10-15/):

| File | What it is |
|---|---|
| `brief.md` | Product and architecture brief: vendor-login verdicts, bridge catalog, positioning, security design, stack, roadmap, and open questions |
| `feature-matrix.json` | 201 features scored across 9 products, each with Lockdown's target (parity, beat, or novel) and phase |
| `verification.json` | Fact-check verdicts on 45 load-bearing claims |
| `report.html` | The interactive report page (the matrix is embedded) |
| `reports/` | The seven underlying research reports |

Where the fact-check disagrees with a research report, the fact-check wins.
