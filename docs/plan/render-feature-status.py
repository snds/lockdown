#!/usr/bin/env python3
"""Render 12-feature-status.md from feature-status.json.

Update a row in feature-status.json when a slice lands, then run
`python3 docs/plan/render-feature-status.py` from the repo root.
"""
import json
from collections import Counter, OrderedDict
from pathlib import Path

here = Path(__file__).parent
data = json.loads((here / "feature-status.json").read_text())
rows = data["features"]

MARK = {"done": "x", "partial": " ", "not-started": " ", "n/a-yet": " "}
WORD = {"done": "Done", "partial": "Partial", "not-started": "Not started", "n/a-yet": "Not code yet"}
ORDER = ["done", "partial", "not-started", "n/a-yet"]

cats = OrderedDict()
for r in rows:
    cats.setdefault(r["category"], []).append(r)
total = Counter(r["status"] for r in rows)

out = []
out.append("# Lockdown: v1 feature status\n")
out.append(f"_Generated from `feature-status.json` (as of `{data['as_of']}`) by `render-feature-status.py`; edit the JSON, not this file. "
           f"The feature list is the {len(rows)} v1 rows of {data['source']}._\n")
out.append("**Done** means it works end to end in today's surfaces (core, desktop, CLI, server) with tests. "
           "**Partial** means some layer exists and the row says what is missing. **Not started** means nothing in code yet. "
           "**Not code yet** is work that is not software (an audit, a licence call, a bug bounty).\n")
out.append("| | Done | Partial | Not started | Not code yet |\n|---|---|---|---|---|")
out.append(f"| **All {len(rows)} v1 features** | **{total['done']}** | **{total['partial']}** | **{total['not-started']}** | **{total['n/a-yet']}** |")
for c, rs in cats.items():
    k = Counter(r["status"] for r in rs)
    anchor = "".join(ch for ch in c.lower().replace(" ", "-") if ch.isalnum() or ch == "-")
    out.append(f"| [{c}](#{anchor}) | {k['done']} | {k['partial']} | {k['not-started']} | {k['n/a-yet']} |")
deferred = sum(1 for r in rows if r.get("deferred"))
out.append(f"\n{deferred} rows are deferred by D122 (OS-level integration and the native mobile apps); they stay on the list, marked.\n")
out.append("## Where things stand\n")
out.append(data["notes"] + "\n")
for c, rs in cats.items():
    out.append(f"## {c}\n")
    for r in sorted(rs, key=lambda r: ORDER.index(r["status"])):
        review = " _(under review)_" if r["review"] else ""
        if r.get("deferred"):
            review += " _(deferred: " + {"os": "OS-level", "mobile": "native mobile"}[r["deferred"]] + ", D122)_"
        where = ", ".join(r["surfaces"]) if r["surfaces"] else "nowhere yet"
        out.append(f"- [{MARK[r['status']]}] **{r['name']}**{review}: {WORD[r['status']]} · {where}. {r['left']}")
    out.append("")
if data.get("non_v1_built"):
    out.append("## Built ahead of its phase\n")
    for r in data["non_v1_built"]:
        out.append(f"- `{r['id']}`: {r['note']}")
    out.append("")
out.append("Evidence (file and line, test, or decision row) for every row is in `feature-status.json`.\n")
(here / "12-feature-status.md").write_text("\n".join(out))
