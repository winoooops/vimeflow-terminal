# Dynamic Island Tab Bar — M2 Design

**Date:** 2026-08-30
**Status:** Codex-reviewed (4 units, 10 rounds via live reviewer pane,
2026-08-30/31); pending PR
**Tracking:** fork GH issue #11 · Linear VIM-428 (decision log in its comments)
**Seam:** `render_tab_bar` + `TabBarView` (`src/ui/tabs.rs`), overlay pattern
(`src/ui/menus.rs`), stock toast pipeline
**Lineage:** vimeflow "Session Island" (`~/projects/vimeflow`,
`docs/superpowers/specs/2026-07-20-session-island-design.md` and
`2026-07-31-in-app-notifications-design.md`)

## 1. Summary

The fork's chrome row replaces the labeled tab bar with the **tab island**: a
capsule showing one marker per tab of the active workspace — active tab as an
elongated pill, the rest as dots — in three display modes
(`dots | numbers | labels`), positioned centered or left, with notification
features attached: an unread bell, a history panel hung from the capsule, and
exactly one sanctioned cross-workspace jump (clicking a record). The island is
the **default** tab bar (`ui.tab_bar_style = "island"`); the upstream labeled
bar stays reachable through the `"classic"` escape hatch. v1 keeps notification
records private inside `AppState` and surfaces arrivals through the existing
toast pipeline; the in-capsule toast stage and grow/shrink motion are a later
experiment phase (M2c), and API/CLI exposure of records is deferred with them.

## 2. Non-goals (v1)

- **No morphing or animation.** The original's reduced-motion mode is the
  TUI's native form; stepwise width animation is the M2c experiment track,
  behind a kill switch, not part of M2a/M2b.
- **No in-capsule toast stage.** v1 arrivals ride the existing toast
  pipeline (tweaked for island records); the `Toast` stage joins the machine
  in M2c.
- **No API, CLI, or persistence for records.** `island_records` is
  presentation state in `AppState` — physically server-side already, but not
  exposed; a later phase adds `notification.*` API methods and the CLI
  without moving the data.
- **Markers never encode agent state.** The island palette is positional
  (pill / solid / dim), ported from the original's explicit rule; agent
  state stays in the compact rail, cards, and pane headers. Marker colors are
  not configurable per state.
- **Dots never switch workspaces.** The only cross-workspace transition the
  island may trigger is opening a notification record.
- **No PUA or emoji in built-in glyphs.** The bell substitute is ASCII
  (VIM-427's font lesson); a width-gated override key allows a Nerd Font
  bell, with a short docs instruction — never a default.
- **Deliberate default flip.** Unlike VIM-427, stock rendering changes by
  default: the island *replaces* the labeled bar. This is an intentional
  opinionated-fork decision (recorded 2026-08-31); `"classic"` preserves the
  upstream bar byte-identically for anyone who wants it.
- **Desktop-only in v1.** The mobile layout builds no tab-bar geometry and
  never calls `render_tab_bar` (`src/ui.rs:225-227,373-385`); the island and
  its history panel are declared desktop-only until a mobile entry point is
  designed.
- **No drag-reorder of island markers in v1.** Tab dragging survives under
  `"classic"`; on the island it is a known, documented regression to revisit
  (markers are 1–2 cells; the original had no reorder either).
- **Untouched:** sidebar (expanded and compact rail), workspace glance,
  pane surfaces, and the full upstream tab bar — scroll buttons, drag,
  right-click, `hide_tab_bar_when_single_tab` — under `"classic"`.

## 3. Lineage and research basis

The Electron original is the **Session Island** — a session switcher and a
four-stage notification center sharing one morphing capsule, itself modeled
on the Noctalia (KDE) pill control (operator-confirmed visual reference,
2026-08-31: dark stadium, wide active pill, dot markers, in-capsule bell
with unread badge). The contract
worth porting, verified in its source and specs:

- Positional three-tone palette (active pill / before / after-dimmed),
  spec-explicit that colors never encode status.
- Stable block pagination (never a sliding window); numbers are global
  positions.
- Stage machine `pill → badge → toast → panel` with mutually exclusive
  strip/notification content; the bell does not exist in `pill`.
- Strict read semantics: only row click, Open, or Mark-all mark read.
- Records: in-memory, cap 50, deduped `(pty, key)`, pruned with dead panes;
  producers fire for background targets only, with a 750ms turn-complete
  settle. The original derived **five** reasons from rich renderer events;
  herdr's server signals are coarser — `Idle|Working|Blocked|Unknown`
  transitions today; BEL is not wired and retained OSC 9;4 feeds detection,
  not events — so v1 defines a reduced taxonomy derived only from signals
  that actually exist (§4.6) — the settle delay is
  an island-owned producer constant, unrelated to the whole-second
  configurable toast delay.
- All motion collapses to 1ms under reduced motion — the TUI inherits that
  form natively.

The `herdr-dynamic-island` archive repo (frozen SPEC.md, phase-0 findings) is
gone from GitHub; its decisions survive quoted in Linear VIM-428, and the
"plugin path impossible" conclusion was re-derived against the current public
API surface: plugins get events, stock toasts, sidebar tokens/views, and
plugin panes — nothing can write into the chrome row or overlay the client
frame, so the island must live in the fork's renderer.

## 4. Product decisions (1–5 resolved 2026-08-31; 6–9 resolved in codex
review round 1)

1. **Replace.** The island is the fork's tab bar. `ui.tab_bar_style =
   "island"` is the default; **`"classic"`** renders the upstream bar
   unchanged. (The escape value is deliberately not called `labels` — that
   word belongs to the island display mode below.)
2. **Position configurable.** `island.position = "center" | "left"`;
   default `"center"` (the island's identity; `"left"` sits where labeled
   tabs start today).
3. **ASCII first, Nerd Font by choice.** Display modes live at
   `island.display = "dots" (default) | "numbers" | "labels"`. The built-in
   bell substitute is `!` (quiet → accent when unread → error tint when an
   unread error exists), count beside it capped `9+`. `island.bell` accepts
   a width-1-or-2 override under the same measured-width gate as
   `compact_rail_marks`; `docs/next` gains a two-sentence Nerd Font
   instruction.
4. **Arrival surface: the toast renderer, island's own policy.** Stock
   toast *delivery* defaults off and is scoped to its own sources
   (`src/config/model.rs:61-69`, `src/app/actions.rs:3248-3262`), so island
   records do not ride that policy: `island.arrivals = "toast" (default) |
   "silent"` routes records through the toast *renderer* directly. The
   in-capsule takeover and expand/shrink feel are M2c experiments.
5. **Records server-owned later, private now.** v1 stores them in
   `AppState` only; the deferral is API exposure, not location. (5a
   corollary: since `AppState` lives in the headless server, the later
   phase adds `notification.*` API methods over the same store.)
6. **v1 reason taxonomy — only what the server can actually see.** Core:
   `turn-complete` (Working→Idle/Done transition, 750ms settle) and
   `blocked` (→Blocked; herdr cannot distinguish approval from question, so
   the original's two reasons merge). Conditional M2b stretch, each gated on
   its producer actually existing: `agent-error` (only if the embedded
   watcher lifecycle exposes an error state) and `terminal-attention` (only
   if a BEL/notify callback gets wired — none is registered today,
   `src/ghostty/mod.rs:805-839`, and retained OSC 9;4 feeds detection, not
   an attention event). The taxonomy enum ships extensible; mapping table
   in §6.
7. **Interaction parity on the island** (amended 2026-08-31, operator
   minimal-design override): the island carries **no `+` button** — the
   capsule holds only markers (and, from M2b, the bell), matching
   Noctalia's minimalism; new tabs come from keybinds and the tab context
   menu, and `"classic"` keeps its `+` untouched. Right-click on a marker
   opens the same tab context menu as a labeled tab; block pagination
   replaces scroll buttons; drag-reorder is v1-deferred (see §2).
8. **Row visibility under `hide_tab_bar_when_single_tab`.** That option
   defaults to `false` (`src/config/model.rs:1063`), so by default the
   island row always renders and nothing changes. When a user enables
   hiding, island style hides the row only when `tabs == 1 AND
   island_records.is_empty() AND !panel_open` — *any* records (read or
   unread) keep the row, so history is never unreachable while records
   exist.
9. **Keyboard-complete panel.** With `ui.mouse_capture = false` the island
   must stay operable: a binding opens/closes the panel, list navigation
   reuses the existing menu keyboard model (up/down/enter/esc), enter on a
   row = the sanctioned jump, `r`/`c` for mark-all-read/clear. Settled in
   §6: `island_panel_toggle`, default `prefix+i`.

## 5. Tab island design (M2a)

### Rendering branch

`render_tab_bar` becomes a two-arm dispatch on the mirrored
`ui.tab_bar_style`: `Classic` runs the current body untouched; `Island`
renders the capsule. Geometry stays inside the existing chrome-row rect
(`view.tab_bar_rect`). One layout-adjacent change exists by design: the
row-existence predicate in `desktop_tab_bar_and_terminal_area`
(`src/ui.rs:194-216`) gains decision 8's island condition
(`tabs > 1 OR !records.is_empty() OR panel_open`) — `src/ui.rs` is already
a registered fork-modified file. Nothing else about layout changes.

### Capsule anatomy (left to right)

```
  ●  2  ● ●             numbers mode, active tab 2, position center
  |  |
  |  └ pill: accent bg, panel_contrast_fg — same pair as today's
  |    active labeled tab
  └ markers: 1 cell each, separated by 1 space; the whole capsule sits
    on a surface0 bg run with 1 space padding each side, distinguishing
    it from panel_bg (no box-drawing in a 1-cell row)
```

- **Markers per display mode** (`island.display`) — amended 2026-08-31
  for the Noctalia-minimal look: the active pill is a **pure solid shape,
  never a glyph**:
  - `dots`: inactive = `⬤` (U+2B24, single cell — a true circle at any
    font metric, ~80% cell height, faithful to the reference's
    smaller-than-pill dots; chosen over the 2-cell semicircle composition
    whose roundness was font-aspect-dependent, 2026-08-31); square caps
    keep `●`. Active = a **3-cell solid accent run** (5 with caps; empty
    spaces on accent bg — color is the content).
  - `numbers` (amended 2026-09-01): inactive = a **mini stadium holding
    the digit** — cap + digit(s) + cap, a muted existing-token bg with
    the positional tone carried by the digit fg — so numbers speak the
    same round language; bare digits remain only under square caps.
    Active = ` N ` (one-space padding) on accent bg with caps as
    everywhere.
  - `labels` (amended 2026-09-03, operator decision): **every tab shows
    its name** — the full labeled bar. Inactive = the tab's display name
    (custom name, else the index as upstream's default text), truncated
    to fit `LABEL_MAX_WIDTH`, in the positional tone; under round caps it
    wears the same mini-stadium as numbers-inactive (cap + name + cap on
    the muted token bg), under square caps bare text. Active = ` name `
    (one-space padding) on accent bg, total width `clamp(3, 16)` cells
    via `truncate_end`, index fallback when unnamed. The previous compact
    labels look is **retired**, closely approximated (not byte-identical:
    the circle stays in the pill, and unnamed fallbacks differ) by
    `dots` + `active_title` — a deliberate trade, no escape-hatch key.
    **Pagination does not change**: `marker_budget` takes
    `active.max(inactive)` and labels' active `LABEL_MAX_WIDTH` already
    dominates, so page math was label-bar-conservative all along; the
    inactive branch's stale `1` merely becomes semantically moot.
    **Inactive label stadiums follow numbers-inactive exactly**: the same
    muted-token mini-stadium, the same zero-padding cap nesting against
    the capsule edge, and the settled + animated `inactive_number_stadium`
    predicates generalize to cover labels. **The narrow-width dots
    fallback remains** as the explicit exception: when the labeled
    endpoints cannot share a page (settled layout or motion endpoints),
    the existing labels-to-dots degradation applies rather than the bar
    vanishing. Motion, endpoints, and hit areas otherwise need no model
    change: marker widths already flow from measured layout text.
  - Capsule padding under round caps is **per-side conditional**
    (amended 2026-09-01 after the flush look): **0** on a side whose
    adjacent element is the active pill's cap — curves nest — and **1**
    on a side whose adjacent element is a circle, digit, label dot, or
    the page indicator, so no glyph is ever crowded by the capsule cap.
    Page-math budgets conservatively reserve 1 per side (stable-input
    rule). Square caps keep 1 cell both sides.
- **Positional palette from existing tokens only**: active = `p.accent` bg
  + `panel_contrast_fg`; markers before the active = `p.overlay1` fg;
  after = `p.overlay0` fg (the dim tone the rail already uses). No new
  palette entries, no state colors (decision §2/§4).
- **Rounded silhouette** (amended 2026-08-31, operator decision after the
  first live look): the capsule and the active pill get semicircle end
  caps — the Powerline glyphs U+E0B6/U+E0B4 drawn as foreground-on-row-bg
  color runs, the standard TUI stadium technique. Ghostty, Kitty and
  WezTerm synthesize these glyphs natively without a patched font; for
  other terminals `island.caps = "square"` restores the flat block look.
  Default `"round"` — a deliberate exception to the ASCII-first rule,
  justified by native terminal synthesis being widespread and the escape
  hatch being one key. Caps cost 2 cells on the capsule and 2 per active
  pill; the width budgets and `markers_that_fit` account for them.
- **Bell region** appears only when `!island_records.is_empty()` (M2b;
  §6): `!` + count, 3–4 cells at the capsule's trailing edge.

### Position, width, batching

- `island.position = "center"`: capsule x-centered in the row rect,
  clamped so it never clips; `"left"`: starts at the row's left edge like
  labeled tabs today.
- Width = markers + separators + padding (+ bell). If it exceeds the
  row, batching engages early (below).
- **Batching** (original's stable-page rule, width-adaptive): page size =
  `min(10, markers_that_fit)`. Worst-case marker budgets depend only on
  stable inputs, never the active tab: `dots` budgets 3 cells (the pill);
  `numbers` budgets `digits(tabs.len()) + 2` cells (tabs are uncapped, so
  position 100 costs three digits); `labels` always budgets the full
  16-cell active clamp. The indicator budget is
  non-circular: if all markers fit with no indicator, one page, no
  indicator. Otherwise reserve `2·digits(tabs.len()) + 3` cells for the
  `‹p/P›` indicator — `tabs.len()` is a true upper bound on total pages
  (page size ≥ 1), so the reservation depends only on the tab count and
  the rect, never on the page count it produces — then compute
  `markers_that_fit` from the remaining width. Block boundaries therefore
  cannot move when switching tabs, and the capsule never clips even on
  narrow desktops (a 65-column terminal leaves ~39 tab-row cells). Pages
  are stable blocks; the page containing the active tab renders.
  Whenever the computed page size changes (resize, mode change, tab count
  crossing a fit boundary) the batch start is re-floored to the new page
  size — the original shipped this exact bug and fixed it
  (`derived-state-consistency.md` #449). No per-marker scrolling, ever.

### Motion (amended 2026-09-01 — pulled forward from M2c as the next slice)

`island.motion = "smooth" (default) | "steps" | "off"` animates **active-tab
changes within the current page** under island style. Page changes, style
flips, and workspace switches always cut — never slide.

- **smooth** (re-based 2026-09-02 on the motion technical review): a
  **critically-damped spring** per participant width (plus one for the
  labels-mode capsule total) replaces duration+easing entirely — position
  and velocity stepped at the 16ms tick, stiffness tuned so an
  uninterrupted switch settles in ≈120–150ms; a tab switch mid-flight
  **retargets** the springs, carrying velocity for the tabs they represent
  (a reversal continues both springs; a switch onward to a third tab keeps
  the departing tab's spring and starts the new tab from its settled
  geometry) — no restart, no jank;
  settle = all springs within epsilon of target with negligible velocity.
  The outgoing pill shrinks toward its resting marker while the incoming
  marker grows to the pill **in the same frames**. Width conservation is
  mode-dependent: in `dots` and `numbers` the settled marker widths are
  fixed, so the swap conserves total capsule width; in `labels` the two
  settled totals differ, so the **capsule width itself tweens** between
  them on the same curve (the island breathes — closer to the reference,
  not a defect). **Shapes never break** (amended 2026-09-02 after the
  first motion look: flat mid-frames read as sharp-corner→pill popping):
  interpolated widths quantize to whole cells and **both participants
  carry their semicircle caps on every frame** — sub-cell eighth-block
  edges are abandoned, since a fractional edge cannot carry a cap without
  a seam. Perceived smoothness comes from the color crossfade (both
  participants' colors lerp between accent and their positional tone —
  truecolor only, wire always emits RGB) riding the ease-out width steps.
  Incoming digit/label content appears only above ~60% progress so text
  is never squashed; mini-stadiums morph like the pill.
- **steps**: the same spring-driven, whole-cell, always-capped morph
  without the color crossfade or velocity ramp.
- **off**: instant switching, byte-identical to the pre-motion behavior —
  the reduced-motion escape this spec has promised throughout.

Mechanics (spring re-base): `island_anim: Option<IslandAnim { from_tab,
to_tab, outgoing_width, incoming_width, capsule_total }>` in `AppState`
(pure presentation), where each field is an `IslandSpring { position,
velocity }` — **spring state is stored**, advanced by ~16ms ticks in both
run modes: the headless loop and the monolithic `App::run` loop share the
deadline source (`island_animation_tick_deadline` in the common deadline
array) and both call `tick_island_animation`; render stays a pure
function of `&AppState`. The animated capsule's non-participant width
interpolates between its from/to endpoint values on the capsule spring's
travel, so conditional endpoint padding differences never snap. **Settled vs visual geometry
are distinct functions**: `layout()` remains the settled geometry and
feeds `compute_tab_bar_view`/hit areas unchanged; rendering calls an
animated overlay (`layout_animated(app, area)` reading the stored
spring positions) that interpolates from settled endpoints. **Animation never
affects logic**: page math, batching, and hit areas use settled
destination geometry from the first frame (clicks during motion behave as
if the switch completed). The anim clears instantly to settled state on:
natural settle, page change, style flip, **any change to `[ui.island]`
config (including `motion` itself) via live reload**, and **any tab
topology change in the active workspace** (create/close/reorder) — which
also makes the index-based `from_tab`/`to_tab` safe for the animation's
short life.

Tests: spring unit table (fixed-dt settle bounds, retarget carries
velocity, no overshoot at critical damping); width conservation
(dots/numbers) and capsule tween (labels); cell-exact frames at settled
endpoints plus capped mid-frames; the no-rect frame sweep;
hit-areas-snap-immediately; `off` byte-identical; steps-mode coverage;
topology clears (tab create/close/reorder, `layout.apply`, `pane.move`).

### Active tab title (amended 2026-09-03 — the next slice)

The active pill in `dots` and `numbers` displays carries the tab's title
text beside its identity mark, converging on the treatment `labels`
already gives its active tab while keeping each mode's inactive identity
untouched.

- **Composition** (amended after operator decision, 2026-09-03): dots:
  `␣⬤␣title␣` — the circle glyph rendered in `panel_contrast_fg` on the
  accent run, so the mode's shape language stays visible inside the pill.
  numbers: `␣title␣` — the title **replaces** the index entirely, never
  `␣N␣title␣`: upstream herdr's index is nothing but the default text an
  unnamed tab falls back to, so a real name supersedes it (the unnamed
  fallback below preserves that original logic exactly). Inactive markers
  are unchanged in both modes. `labels` display ignores the key — it
  already titles the active tab and shows no mark.
- **Title source and fallback**: `tab.custom_name` directly — the tab's
  stored name, whether user-set (rename, `tab.create` label, worktree
  label) or auto-assigned, and **nullable**. NOT `tab_display_name`,
  whose index-string fallback would render the explicitly rejected
  degenerate `2 2` in numbers. `None` renders exactly today's untitled
  form (`␣␣␣` solid run / `␣N␣`); the mark already identifies the tab.
- **Width budget**: the whole active pill (mark, spaces, title, padding,
  before caps) clamps to the existing `LABEL_MAX_WIDTH = 16` via
  `truncate_end`, minimum the current untitled width. `marker_budget`
  currently budgets the dots active at 3 and numbers at `digits + 2`;
  with `active_title` on, **both modes budget the active at
  `LABEL_MAX_WIDTH`** (the labels rule) so the stable-input page math
  never admits more markers than the titled pill leaves room for — the
  cost is labels-equivalent pagination, never an island that overflows or
  vanishes. On rows too narrow for any title, truncation walks the title
  down and the pill bottoms out at its untitled form rather than forcing
  a different page plan.
- **Motion interplay**: endpoints derive from layouts, so the incoming
  pill's spring targets the titled width and the outgoing shrinks from
  it. This **retires the dots/numbers width-conservation property**: with
  per-tab title widths the settled totals differ between endpoints, so
  titled dots/numbers breathe through the capsule tween exactly like
  labels (the Motion section's conserved-total fallback chain already
  handles the equal-width special case). Mid-flight content keeps the
  existing >60%-activation gate (`animated_content_visible`);
  `render_animated_content` drops its dots early-return so a titled dots
  pill draws its content the same way numbers and labels do.
- **Title changes without a tab switch** snap, never stretch: every
  `custom_name` mutation on the active workspace (rename API, TUI rename
  modal, worktree label flows) refreshes the tab bar view and clears any
  in-flight animation whenever the island is actually rendering titles —
  the existing labels-only rename guard widens to
  `display == labels || active_title`. Same rationale as the rename rule:
  springs must not settle against pre-change widths.
- **Config**: `[ui.island] active_title = true` (default) `| false`,
  live-reloadable; the existing island-config-change clear already snaps
  any in-flight animation when it flips. Because a derived
  `IslandConfig::default()` would give a bare bool `false`, the field
  needs an explicit `#[serde(default = ...)] = true` (plus matching
  `Default` impl) and an absent-key test asserting it parses to `true`.
  `false` restores the pure-solid pill and bare `␣N␣` exactly.
- **Hit areas**: automatic — marker hit rects come from layout widths.

### Hit areas and input

- `TabBarView` gains island fields computed in `compute_tab_bar_view`:
  `island_marker_hit_areas: Vec<Rect>`, `island_bell_hit_area: Rect`. All
  mouse routing joins the current chrome handling in
  `src/app/input/mouse.rs`.
- Left-click marker → focus that tab (existing action, keeps terminal
  focus). Right-click marker → the same tab context menu a labeled tab
  opens today (decision 7). No hover tooltips in v1 — `labels` mode is the
  see-the-name affordance; herdr has no tooltip surface.
- Keyboard: no new marker navigation — existing tab keybinds already
  cover switching; the island only reflects state (lineage rule).

### Tests (M2a)

Buffer assertions per the collapsed-rail test pattern: capsule cell-exact
render in all three modes at representative widths; center and left
positioning; positional three-tone assignment around the active index;
batching at 11+ tabs; `classic` byte-identical to today's output;
`hide_tab_bar_when_single_tab` matrix per decision 8; hit-area geometry
tests beside the existing `TabBarView` tests; live config flips for
`tab_bar_style`, `island.position`, `island.display`. Active-title slice:
marker composition per mode (titled, untitled fallback, truncation at the
16-cell clamp), page math with a widened active pill, the dots
animated-content gate, and the `active_title` live flip restoring the
untitled forms byte-identically.

## 6. Notifications v1 design (M2b)

### Record model (AppState, private in v1)

```rust
struct IslandRecord {
    id: u64,                       // monotonic
    workspace_id: String, tab_id: String, pane_id: PaneId,
    agent: Option<crate::detect::Agent>,
    reason: IslandReason,          // extensible enum, decision 6
    text: String,                  // e.g. "codex turn complete"
    at: std::time::SystemTime,
    read: bool,
}
```

`island_records: VecDeque<IslandRecord>` — newest first, cap 50, dedupe
`(pane_id, reason)` with newest-wins refresh, pruned when the pane closes
(same sweep that clears pane state today). A refresh is a new event: the
superseded record is removed and the replacement gets a **new monotonic
id**, `read = false`, fresh `text`/`at`, and the front position — so
unread counts and toast record-id activation always reference the
replacement.

### Producer mapping (decision 6)

| herdr signal | reason | gate |
| --- | --- | --- |
| `AgentState` Working → Idle/Done, stable for 750ms | `turn-complete` | background target only |
| `AgentState` → Blocked | `blocked` | background target only, immediate |
| watcher lifecycle error state | `agent-error` | M2b stretch — only if the watcher exposes it |
| BEL / notify callback | `terminal-attention` | M2b stretch — only if wired |

Background target = the pane is not the focused pane, or its workspace is
not active — the original's rule mapped onto herdr identity. Producers
hook where agent state transitions are already applied in the app layer
(the same place `state_change_seq` advances), so no new event plumbing.

### Stage machine

Stage is derived, never stored:
`if island_records.is_empty() { Pill } else if island_panel_open { Panel }
else { Badge }` — M2c later inserts a `Toast` arm. **Invariant:**
`island_records.is_empty() ⇒ !island_panel_open`; every path that can
empty the store (clear-all, the prune sweep) also resets the flag, so an
emptied panel closes and the next arrival lands in `Badge`, not `Panel`. Transitions (all in
`actions.rs`; render stays pure):

| event | action |
| --- | --- |
| record arrival | push + dedupe; if `island.arrivals = "toast"`, emit through the toast renderer with island-owned delivery (decision 4), mapping reasons onto the existing `ToastKind` (`src/app/state.rs:1305`): `turn-complete → Finished`, `blocked → NeedsAttention`, stretch reasons → `NeedsAttention`; no new kind in v1, so colors and lifetimes are the kinds' standard ones. The island toast sets **no ordinary `target`** (so `open_notification_target`/`prefix+o` never touches it, `src/app/input/navigate.rs:654-673`); instead it carries the record id, and its click activation routes through the open-record action below (mark read + jump) |
| bell click / panel keybind | toggle `island_panel_open`; **no-op while `island_records.is_empty()`** (preserves the invariant; matches the original, whose bell does not exist in `pill`) |
| record row open (click or Enter) | mark read → focus workspace → tab → pane (existing actions; the one sanctioned cross-workspace jump; immediate — no 160ms delay, nothing animates yet) |
| Esc / outside click | close panel — see dispatch note below |
| mark all read / clear all | `read = true` for all / `records.clear()` **+ `island_panel_open = false`** → Pill |
| pane closed | prune referencing records; if the store empties, reset `island_panel_open` (invariant above) |

### Panel

**Dispatch:** input handling in herdr is Mode-dispatched
(`src/app/input/mod.rs:93`); there is no generic menu-dismiss layer. The
panel therefore follows the context-menu pattern exactly: `island_panel_open`
is checked at the same priority tier as the context-menu state in both key
and mouse dispatch (before pane input), Esc and outside-click close it
there, and rendering joins the overlay pass beside `render_context_menu`
(`src/ui.rs:434` area). No new `Mode` variant.

**Geometry:** rendered with the `menus.rs` `Clear` + `List` overlay
pattern, anchored at the capsule's x-center, opening **toward the panes**
— downward when `ui.tab_bar_position = "top"`, upward when `"bottom"`
(both positions are supported today, `src/config/model.rs:810-815`).
Width `min(48, row_width - 4)` cells, max 6
rows + list scroll. Row: `● text · wN:tM · age` (`age` = `now/Nm/Nh`,
original's vocabulary); read rows render dim; header (fits the 46 inner cells of the bordered shell):
`N unread · [r]read · [c]clear` — the word "notifications" is carried by
the bell the panel hangs from, not the header. No per-row
dismiss in v1 (clear-all only — a documented simplification of the
original's hover `✕`). Keyboard: action id `island.panel.toggle`, serialized in `[keys]` as
**`island_panel_toggle`** following the existing snake_case action names
in `src/config/keybinds.rs` (default chord
**`prefix+i`** — free in the stock table, where `prefix+n` is `next_tab`
and `prefix+o` is `open_notification_target`) and
`island.panel.open_record`. `prefix+o` keeps its behavior untouched; it
never interacts with island records because island toasts carry no
ordinary target (see the arrival row above). Users rebind
`island.panel.toggle` through the normal keybinds mechanism; no stock
conflict exists since the default claims a free chord. Up/down/enter/esc
follow the menu keyboard model (decision 9).

### Style gating and live transitions

The notification subsystem is **island-only**. Under
`tab_bar_style = "classic"` no producers run, the `prefix+i` binding
no-ops, and no bell or panel exists (there is no capsule to anchor them).
A live switch island → classic closes the panel and suspends producers;
existing records are retained dormant and reappear on switching back —
a config flip never destroys data. classic → island resumes producers
from the next transition.

### Bell

`!` + unread count (`9+` cap), fg `overlay0` quiet → `accent` unread →
error tint only if an unread `agent-error` exists (stretch).
`island.bell` override: width-gated 1–2 cells, the `compact_rail_marks`
validation pattern verbatim.

### Tests (M2b)

Record store unit tests (cap, dedupe-refresh, prune); producer gating
(background-only, settle debounce) against `AppState::test_new`
scenarios; stage derivation truth table; panel buffer assertions (rows,
dim read state, header) and hit areas; keyboard transcript tests via the
existing menu-test pattern; jump action sets workspace + tab + pane and
marks read; `island.arrivals = "silent"` produces no toast.

## 7. Config design

```toml
[ui]
tab_bar_style = "island"   # "island" (default) | "classic"

[ui.island]
position = "center"        # "center" | "left"
display  = "dots"          # "dots" | "numbers" | "labels"
caps     = "round"         # "round" | "square"  (silhouette; round = powerline semicircles)
motion   = "smooth"        # "smooth" | "steps" | "off"  (within-page tab-switch morph)
active_title = true        # show the tab title beside the active mark (dots/numbers)
arrivals = "toast"         # "toast" | "silent"        (M2b)
bell     = "!"             # 1-2 cell string override  (M2b)
```

- **No inherit dance this time.** Unlike VIM-427's optional key, the island
  is the default (decision 1), so these are plain fields with serde
  defaults — `TabBarStyleConfig` defaulting to `Island`, an `IslandConfig`
  struct with `#[serde(default)]` — mirroring `compact_rail_numbers`'s
  startup + live copy pattern into `AppState`. Every key reloads live,
  including `tab_bar_style` itself (it is only a render branch).
- **Validation**: enum values serde-reject unknown strings (safe error);
  `bell` gates on measured display width 1–2 with right-padding, invalid
  values fall back to `!` with a startup/live diagnostic through the
  `compact_rail_marks` diagnostic path in `src/config/io.rs` — same
  wording style.
- **Docs (same PR as each phase)**: `config-reference.json` entries with
  `values` arrays for **all four enums** (`tab_bar_style`, `position`,
  `display`, `arrivals` — the checker requires exact arrays, learned on
  VIM-427); a tab-island section in `docs/next` **`configuration.mdx`**
  (first fork edit of that file — new registry row expected, see §9); and
  decision 3's two-sentence Nerd Font instruction beside the `bell` key:
  install any Nerd Font, set it as the terminal font, then e.g.
  `bell = "󰂚"`.

## 8. Validation

Per-slice tests live in §5/§6. Cross-cutting gates, all existing:
`just check` (fmt, clippy, nextest, windows-lint — island code is
cross-platform, zero `cfg` gating), `config_reference_check.py` with the
new keys, bun integration assets untouched, and the buffer-assertion
suites beside the current `TabBarView`/collapsed-rail tests. Live GUI
verification per phase against a dev server (`HERDR_SOCKET_PATH` pinned —
the watcher-socket coexistence lesson), using a throwaway
`HERDR_CONFIG_PATH` config.

## 9. Phasing, module placement, and registry impact

**Module placement minimizes the upstream diff and respects the pure-UI
boundary**: two new **fork-added** modules — `src/ui/island.rs` (capsule
and panel rendering + geometry helpers only; pure functions of
`&AppState`) and `src/app/island.rs` (record store type, producers, and
mutation helpers, invoked from `actions.rs`) — listed under "Fork-added
files", carrying no 4(b) burden. The `AppState` fields themselves land in
`src/app/state.rs` (registered). Registered upstream files receive only
dispatch-sized edits:

| File | Edit | Registry status |
| --- | --- | --- |
| `src/ui/tabs.rs` | two-arm `tab_bar_style` dispatch in `render_tab_bar` + island fields on `TabBarView` | **new row + 4(b) notice + MODIFICATIONS entry** (first fork edit of this file) |
| `src/ui.rs` | decision-8 row-existence predicate + the `ViewState` copy of the island view fields (`compute_tab_bar_view` result is copied there) | registered ✓ |
| `src/app/actions.rs` | second `compute_tab_bar_view` call site (`:1752`) + island mutation wiring | registered ✓ |
| `src/config/model.rs` | `TabBarStyleConfig`, `IslandConfig` | registered ✓ |
| `src/config/io.rs` | bell diagnostics | registered ✓ |
| `src/app/state.rs`, `src/app/mod.rs` | mirrors + copies | registered ✓ |
| `src/app/input/mouse.rs` | marker/bell/panel hit routing | registered ✓ |
| `src/app/input/mod.rs` (or keys path) | panel priority tier beside context menu | **likely new row** — confirm at implementation |
| `src/config/keybinds.rs` | parse `island_panel_toggle` (M2b) | **new row + notice** (first fork edit) |
| `src/app/input/navigate.rs` | dispatch the rebindable panel action (M2b) | registered ✓ |
| `docs/next` `configuration.mdx` | island section + Nerd Font note | **new row + notice** (first fork edit) |
| `docs/next` `config-reference.json` | four enum entries with `values` | registered ✓ |

**Phases** (each its own PR via the fork workflow — spec → codex
implements → Claude reviews → `just check` → PR `refs #11`):

1. **M2a — the capsule** (this branch): dispatch + island render (dots/
   numbers/labels, positional palette, position, adaptive batching), hit
   areas + click/right-click, the row predicate **in its M2a
   degenerate form** — `island_records`/`island_panel_open` do not exist
   yet, so their terms are constant-false and the predicate reduces to
   `tabs > 1`; the full decision-8 condition lands with M2b's fields —
   config keys (`tab_bar_style`, `position`, `display`), tests, docs.
   `classic` byte-identical.
2. **M2b — bell + records + panel**: record store + producers
   (turn-complete, blocked; stretch reasons only with their producers),
   badge/panel stages, `prefix+i`, arrivals-toast bridge with record-id
   activation, `arrivals`/`bell` keys, tests, docs.
3. **M2c — experiments** (separate exploration before any PR): in-capsule
   toast takeover, stepwise width grow/shrink behind a motion kill-switch,
   pause-on-hover dwell. Findings return to this spec as amendments.

Later, out of M2 scope: `notification.*` API + CLI over the same store
(decision 5a), mobile entry point, marker drag-reorder, per-row dismiss.

<!-- codex-reviewed: 2026-08-31T06:49:53Z -->
