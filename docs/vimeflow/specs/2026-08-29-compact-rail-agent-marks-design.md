# Compact Rail Agent Marks — Design

**Date:** 2026-08-29
**Status:** Codex-reviewed (6 rounds via live reviewer pane, 2026-08-29);
pending PR
**Tracking:** fork GH issue #1 (canonical scope/AC) · Linear VIM-427
**Seam:** `RailLeading::{Number, None}` + `render_compact_rail_row`
(`src/ui/sidebar.rs:840`, fork commit `550b21a4`)

## 1. Summary

The collapsed sidebar's compact agent list gains an opt-in third leading
mode: a **per-agent identity mark** — a two-character ASCII mark derived
from the detected `crate::detect::Agent` (e.g. `Cl` claude, `Cx` codex) —
selected via a new `compact_rail_leading = "agent"` config value (§4
"Config interaction"; full key contract in §6). The mark
renders through the existing `render_compact_rail_row` leading slot — the
same 2-cell field the number mode already reserves — so rail geometry is
untouched. The default mode is unchanged from today. An optional config map
lets users substitute their own string per agent (including single-cell
Nerd Font glyphs); the shipped default is plain ASCII only.

## 2. Non-goals

- **No layout re-derivation.** All rendering goes through
  `render_compact_rail_row`; `COLLAPSED_WIDTH` stays 4 (`src/ui.rs:109`).
- **Workspace glance rows keep numbers.** Glance rows index workspaces, not
  agents; a workspace aggregates several agents, so an agent mark is
  meaningless there. (Resolves the "decide whether the workspace glance
  section keeps numbers" item in GH #1.)
- **No per-agent colors.** The state dot keeps encoding lifecycle by color;
  the mark carries identity only. Per-agent color is a possible follow-up.
- **No Nerd Font glyphs or emoji in built-in marks.** See §3. Built-in
  defaults are plain ASCII always. Non-ASCII (PUA glyphs, emoji, CJK) is
  reachable only through the user override map, which gates on measured
  display width alone (§4) — rendering fidelity of such overrides is
  explicitly the user's responsibility, since terminals disagree on emoji
  and unpatched fonts show PUA as tofu.
- **Expanded sidebar untouched.** Agent cards, legacy rows, and the expanded
  Agents section render as today.
- **Stock behavior byte-identical — for every existing config, not only the
  default.** Marks render only when `compact_rail_leading = "agent"` is set
  explicitly; when the new key is absent, agent rows inherit today's
  `compact_rail_numbers` behavior exactly (GH #1 acceptance; see §4
  "Config interaction").

## 3. Research basis (2026-08-29)

Verified against Nerd Fonts `glyphnames.json` (master) and
`microsoft/vscode-codicons` sources:

- Terminal-usable **brand** glyphs exist for exactly 3 of the fork's 21
  `detect::Agent` variants: `cod-claude` U+EC82, `cod-openai` U+EC81 (fits
  codex), `cod-copilot` U+EC1E. `google-gemini.svg` exists upstream in
  codicons but is not yet in a Nerd Fonts release. `cod-cursor` U+EC5C is a
  mouse-pointer icon, **not** the Cursor brand. The remaining 17 agents have
  no brand glyph anywhere in Nerd Fonts.
- All of these are PUA codepoints: they render only under a patched
  (Nerd) font, which Herdr cannot detect or guarantee — so they can never be
  the shipped default.
- The agent TUIs themselves draw no distinctive plain-Unicode brand marks
  (evidence: the fork's own `src/detect/manifests/*.toml` bottom-buffer
  rules — prompts, braille spinners, arrows). The one exception is Cursor's
  ⬡/⬢ hexagon. AI "sparkle" glyphs (✳ ✦ ✻) are shared across brands and do
  not identify an agent.

Conclusion: identity must default to ASCII letters; glyphs are a per-user
opt-in via config override.

## 4. Mark scheme

Marks are keyed off the `crate::detect::Agent` enum variant — not the
manifest display label — in a single total `match` in `src/ui/sidebar.rs`
(marks are presentation, so per the runtime/client boundary guardrail they
live in the TUI layer, not in `src/detect/`; that file is also already in
the FORK.md registry, so no new upstream-edit row is needed).
Exhaustiveness is compiler-enforced when a new agent variant lands:

| Agent | Mark | | Agent | Mark | | Agent | Mark |
|---|---|---|---|---|---|---|---|
| Claude | `Cl` | | Gemini | `Ge` | | Amp | `Am` |
| Codex | `Cx` | | GithubCopilot | `Gh` | | Antigravity | `Ag` |
| Cursor | `Cu` | | Grok | `Gr` | | Devin | `De` |
| Cline | `Cn` | | Kimi | `Ki` | | Droid | `Dr` |
| OpenCode | `Oc` | | Kiro | `Kr` | | Maki | `Ma` |
| Omp | `Om` | | Kilo | `Kl` | | Mastracode | `Ms` |
| Pi | `Pi` | | Hermes | `He` | | Qodercli | `Qo` |

Rules:

- Two ASCII characters, capital initial + one lowercase distinguishing
  letter; all 21 marks unique (enforced by a unit test over `Agent::ALL`).
- Rows whose entry has no detected agent (`AgentPanelEntry.agent == None`)
  render an empty 2-cell leading field — the state dot stays column-aligned
  with marked rows.
- Rejected alternative — single letters (GH #1 sketch `C/X/K/O`): the
  C×4 / G×3 / K×3 collision clusters force arbitrary non-initials that stop
  being mnemonic at 21 agents; two cells are already reserved by the number
  mode, so the wider mark costs nothing.

Config interaction (decides GH #1's "extend the existing key or add a
sibling"):

- New sibling key `compact_rail_leading`, optional, values
  `"number" | "none" | "agent"`, applying to the **compact agent list rows
  only**. Absent (the default) means *inherit*: agent rows follow
  `compact_rail_numbers` exactly as today (`true` → number, `false` → none).
  This keeps every existing config byte-identical, including
  `compact_rail_numbers = false` users.
- Workspace glance rows always follow `compact_rail_numbers` (per §2 they
  never render marks); the new key does not affect them.
- Mark width rule: the leading field is exactly 2 cells. Built-in marks are
  2 ASCII chars. A user override renders only if its Unicode display width
  is 1 or 2 (per the `unicode-width` crate the renderer already relies on
  through ratatui); width-1 overrides are right-padded with one space so the
  dot column never shifts; anything else falls back to the built-in mark
  for that agent. Measured width is the only gate — an override that
  measures 1-2 renders even if it is emoji or PUA, per §2's
  user-responsibility rule. Serde/TOML contract, validation diagnostics,
  and live reload are specified in §6.

## 5. Rendering design

- New variant, parallel to the existing ones (`src/ui/sidebar.rs:840`):

  ```rust
  enum RailLeading {
      Number { value, field_width, style, gap_style },
      Glyph { text: String, style: Style },   // new
      None,
  }
  ```

  `Glyph.text` arrives already width-normalized to exactly 2 cells (§4), so
  `render_compact_rail_row` renders it exactly like a `field_width: 2`
  number — text span, then the state dot — with no gap span and no new
  geometry.
- Mark resolution happens where the leading value is chosen today
  (`src/ui/sidebar.rs:990`): `entry.agent` (`Option<crate::detect::Agent>`
  on `AgentPanelEntry`, `src/ui/sidebar.rs:35`) → override map lookup →
  built-in mark; `None` agent → `"  "` (two spaces) so the dot column stays
  aligned.
- Mark style reuses the number's current style in the agent list
  (`position_style`, fg `overlay0` — `src/ui/sidebar.rs:988`); the dot and
  its state colors are untouched.
- Workspace glance rows (`src/ui/sidebar.rs:943`) are not modified: they
  continue to branch only on `compact_rail_numbers`.
- `AppState` mirrors the resolved settings the way
  `compact_rail_numbers: bool` is mirrored today (`src/app/state.rs:1483`):
  a `compact_rail_leading` enum (`Inherit | Number | None | Agent`) and the
  validated override map. Rendering reads `&AppState` only. Nothing crosses
  the shared runtime/API surface: no JSON-API fields, no wire-protocol
  changes, no detection changes — the new fields are TUI presentation state
  carried in `AppState` exactly as `compact_rail_numbers` already is
  (runtime/client boundary guardrail).

## 6. Config design

- `[ui.sidebar]` gains two keys beside `compact_rail_numbers`
  (`src/config/sidebar.rs:441`):

  ```toml
  [ui.sidebar]
  compact_rail_numbers = true          # unchanged, workspace glance + inherit
  compact_rail_leading = "agent"       # optional: "number" | "none" | "agent"
  [ui.sidebar.compact_rail_marks]      # optional per-agent overrides
  claude = ""                    # e.g. Nerd Font cod-claude, width 1
  codex  = "CX"
  ```

- `compact_rail_leading`: `Option<CompactRailLeadingConfig>` with
  lowercase serde values; absent → inherit semantics per §4.
- `compact_rail_marks`: string→string table. Keys are the canonical agent
  kind slugs the CLI already exposes (`pi|claude|codex|gemini|cursor|devin|
  agy|cline|omp|mastracode|opencode|copilot|kimi|kiro|droid|amp|grok|
  hermes|kilo|qodercli|maki`). Unknown key or width-invalid value (§4 rule)
  → the entry is ignored with a startup/live diagnostic through the same
  path that diagnoses legacy Agents-row settings in `src/config/io.rs`; the
  built-in mark stays in effect.
- Live reload: both keys apply live exactly like `compact_rail_numbers`
  (startup copy `src/app/mod.rs:625`, live copy `src/app/mod.rs:1463`).
- Docs: `docs/next/website/src/content/docs/agents.mdx` and
  `docs/next/website/src/data/config-reference.json` gain the two keys in
  the same PR (both files already carry fork registry rows).

## 7. Testing & delivery

Tests (all patterned on existing neighbors):

- Mark table: uniqueness + exact display width 2 across `Agent::ALL`
  (new unit test beside the mark `match` in `src/ui/sidebar.rs`).
- Buffer assertions per rail width (pattern: the collapsed-rail buffer
  tests beginning at `src/ui/sidebar.rs:2437`): agent mode renders mark+dot at
  `COLLAPSED_WIDTH`; no-agent row renders blank field + aligned dot;
  workspace glance row is byte-identical with `"agent"` set.
- Inherit matrix: key absent × `compact_rail_numbers` true/false is
  byte-identical to today's Number/None output.
- Config: TOML parse of both keys (pattern `src/config/sidebar.rs:498`);
  invalid-width and unknown-key diagnostics (pattern in
  `src/config/io.rs`); live flip both ways (pattern
  `src/app/mod.rs:3119`).
- Override normalization: width-1 override padded, width-3 rejected with
  fallback.

Delivery per the fork workflow: branch `issue/1-compact-rail-agent-marks`
(no Linear token, per VIM-427), PR against `main` with `refs #1`, commits
propose-then-align, `just check` green before review. All files this spec
touches are already in the FORK.md registry; the PR appends its commit refs
to their rows. Follow-ups deliberately out of scope: per-agent mark colors;
a shipped preset for Cursor's ⬡/⬢; auto-detecting Nerd Font capability.

<!-- codex-reviewed: 2026-08-30T03:46:06Z -->
