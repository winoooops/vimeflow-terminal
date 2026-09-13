<!-- Modified from herdr by the vimeflow project — see FORK.md -->

# vimeflow-terminal

Terminal-native agent runtime for coding agents, built as a tracking fork of
[herdr](https://github.com/herdrdev/herdr). `CLAUDE.md` is a symlink to this
file; edit `AGENTS.md`.

## Read herdr first

This file documents only what the fork adds or changes. The engine, the socket
API, the workspace/tab/pane model, agent detection, plugins, and the project's
universal engineering rules are all upstream's, and upstream's own material is
the reference for them:

- Agent guidance: <https://github.com/herdrdev/herdr/blob/master/AGENTS.md>
- User and API docs: <https://herdr.dev/docs/>
- `FORK.md` — fork base, branch model, upstream-edit registry, merge procedure
- `README.md` — what the fork is, building it, coexisting with an installed herdr

Upstream's *universal* rules apply here unchanged. Its Maintainer Workflow,
Local Can Machine Workflow, Release Channels, and External contributor
guardrail describe the upstream repository and do not apply: no
`origin/master` PRs, no Greptile/CodeRabbit gating, no `just release`, no
upstream issue intake. `.github/MAINTAINERS` lists upstream maintainers, not
this fork's. Wherever upstream's guidance says `herdr <command>`, run
`vimeflow <command>`.

### Upstream rules you will hit daily

A digest so the rules travel with this file; the link above has the full text
and the reasoning.

- State is separated from runtime. `AppState` is pure data, testable without
  PTYs (`AppState::test_new()`, `Workspace::test_new()`); identity and state
  refactors use the test-only `assert_invariants_for_test()` helpers. Render
  is pure and never mutates state; `compute_view()` does geometry.
- Platform code lives in `src/platform/<os>.rs` and is cfg-gated everywhere
  else. Core modules carry no `#[cfg(target_os)]`.
- Runtime/client boundary: shared runtime facts belong in server state and the
  JSON API; TUI presentation state stays in the client layer. Use neutral
  server/API names, never sidebar, row, card, or widget.
- Screen detection is evidence-based. Capture the bottom buffer with
  `vimeflow agent read <pane> --source detection --format text` before
  editing `src/detect/manifests/`, and hot-reload with
  `vimeflow server reload-agent-manifests`.
- Rust: no `unwrap()` in production code, `tracing` for logging, `#[allow]`
  only with a comment, no new dependencies without a reason.
- Commits are lowercase conventional commits with no emojis and no AI
  co-author lines; `refs #<n>` in the body, never closing keywords. Propose
  the commit message before committing.
- Use `just` recipes and run `just check` before committing. Reuse existing
  UI/UX patterns rather than inventing one-off screens.
- Vendored libghostty-vt and portable-pty patches are tracked in registries
  under `vendor/` that `just check` verifies.

## Branch model and workflow

- **`main` is the product branch; `master` is a fast-forward-only mirror of
  `upstream/master`.** Branch from `main`, rebase on `origin/main`, open PRs
  against `main`. Never commit fork work to `master`. Every change lands
  through a reviewed PR tied to a fork GitHub issue; `refs #<n>` in commit
  bodies points at fork issues, and the Linear `VIM-*` IDs quoted in specs are
  mirrors, not targets. Task branches keep upstream's `issue/<id>-<slug>`
  naming. Worktree location is free; Claude Code's worktree tool uses the
  untracked `.claude/worktrees/`, not `../herdr-worktrees/`.
- Fork CI is `.github/workflows/fork-ci.yml`: a `REMOVED_PATHS` reappearance
  guard, then `cargo build --locked` + `cargo nextest run --locked` on Linux
  and macOS for pushes and PRs to `main`. The other workflows are upstream's
  and are gated to the `herdrdev/herdr` repository, so they never run here.
- Editing an upstream file requires the Apache 4(b) notice comment at the top
  of that file plus a row in the `FORK.md` registry and `MODIFICATIONS`. New
  fork-only files go in the "Fork-added files" list. Never recreate a path
  listed in `REMOVED_PATHS`; if upstream grows a real dependency under one,
  narrow the registry in its own reviewed PR.
- Self-update, hosted manifest fetches, and product announcements are
  deliberately neutralized (`src/update.rs`, `src/product_announcements.rs`).
  Do not re-enable them.
- Fork specs, plans, and reviews are tracked in `docs/vimeflow/`, not the
  ignored `.local/prd/`. Upstream's `docs/next/` staging still applies to
  user-facing changes, and fork settings are documented there too. The preview
  and stable snapshot machinery does not apply: `docs/preview/` and
  `docs/versions/` are removed paths.
- `.agents/skills/` holds upstream's project skills. `herdr-throwaway-repro`
  is the one upstream's Agent Detection Updates section refers to and works
  here once `herdr` is read as `vimeflow`. `triage` and
  `herdr-pre-release-audit` target the upstream repository and do not apply.

## Naming

The executable is `vimeflow`; the Cargo package stays `herdr`, so `herdr::`
paths in `tests/` keep resolving.

- Config, state, sessions, sockets, and plugin state live under the app dir
  from `config::io::app_dir_name()`: `vimeflow` for release builds and
  `vimeflow-dev` for debug builds, so `~/.config/vimeflow/` and
  `~/.local/state/vimeflow/`. Nothing is inherited from an installed herdr,
  and the two can run side by side. Upstream's agent-detection override path
  is therefore `~/.config/vimeflow/agent-detection/<agent>.toml` for an
  installed binary and `~/.config/vimeflow-dev/...` for `cargo run`.
- `HERDR_*` environment variables, the `herdr.sock` / `herdr-client.sock`
  filenames, `HERDR_LOG`, and plugin IDs are the integration protocol and are
  deliberately **not** renamed; `herdr-agent-watcher` alone reads nine of
  those variables. Do not rename them.
- Tracing targets follow the binary name, so they are `vimeflow::…` and the
  default filter in `src/logging.rs` names that crate. A `HERDR_LOG` directive
  written as `herdr=…` matches none of the fork's own targets.
- To test a checkout build from inside a running session, clear the inherited
  overrides so the debug binary talks to its own `vimeflow-dev` server:

  ```bash
  env -u HERDR_ENV -u HERDR_SOCKET_PATH -u HERDR_CLIENT_SOCKET_PATH cargo run -- <command>
  ```

## Commands

```bash
just test                 # nextest + python maintenance tests + bun integration-asset tests
just test-one <filter>    # one nextest substring filter, e.g. just test-one codex_stale_working
just lint                 # cargo fmt --check + clippy --all-targets -D warnings
just ci                   # lint + nextest + asset tests (fork CI itself runs build + nextest)
just check                # ci + Windows-target clippy + maintenance script tests
just windows-lint         # catch cfg(windows) breakage from macOS/Linux
just build                # cargo build --release --locked
just default-config       # print the commented default config template
```

`just test` and `just check` need `python3`, `bun`, and `cargo-nextest` on
`PATH`. `just check` cross-compiles for `x86_64-pc-windows-msvc`, so the first
run installs that target. To run one integration binary use a nextest
filterset: `cargo nextest run --locked -E 'binary(watcher_cli)'`.

The toolchain is pinned to Rust 1.96.1 (`rust-toolchain.toml`). `build.rs`
compiles the vendored libghostty-vt with **Zig 0.15.2**, taken from `PATH` or
the `ZIG` environment variable; a newer Zig fails. On a cold Zig cache, run
`scripts/preseed_zig_cache.sh` first — `FORK.md` explains why.

Test-harness facts that are easy to trip over:

- Raw `cargo test` is not the baseline: its shared-process harness trips the
  two known upstream failures documented in `FORK.md`. Use nextest. macOS CI
  excludes one binary: `cargo nextest run --locked -E 'not binary(live_handoff)'`.
- `.config/nextest.toml` puts every integration binary that spawns a real
  server (`api_ping`, `auto_detect`, `client_mode`, `cross_area`,
  `detach_reattach`, `live_handoff`, `multi_client`, `server_headless`,
  `watcher_cli`) in a one-thread test group. Run concurrently they starve each
  other of descriptors and fs watches and time out, so do not fix such a
  timeout by lengthening it.
- `tests/cli.rs`, which bundles the whole `tests/cli/` suite, is compiled out
  on macOS.
- Adding or renaming a config key fails `just test` until
  `docs/next/website/src/data/config-reference.json` matches the serde model;
  `scripts/config_reference_check.py` walks `src/config/*.rs`. Fork-only keys
  must also appear, commented out, under `# Fork-only settings` in the
  `DEFAULT_CONFIG` template in `src/main.rs`, where a unit test checks for
  them.
- macOS caps a Unix socket path at 104 bytes and the fork's app-dir name is
  three bytes longer than upstream's, so tests that bind sockets under a
  temporary root must keep that root short (see `tests/watcher_cli.rs`).

## Architecture

Enough of upstream's shape to place the additions, then the additions.

One binary, three roles, dispatched from `src/main.rs` and `src/cli.rs`. The
**headless server** (`src/server/headless.rs`) owns `AppState`, all PTYs, and
the event loop, renders into an in-memory ratatui `Buffer`, and listens on two
sockets: `herdr.sock`, the public JSON API (`src/api/`, methods declared as the
`Method` enum in `src/api/schema/`, spoken by `vimeflow agent ...`, plugins,
and `skills/herdr`), and `herdr-client.sock`, the private binary TUI protocol
(`src/protocol/wire.rs`, `PROTOCOL_VERSION`). The **thin client**
(`src/client/`) blits diffed frames and forwards input; it holds no
application state. That two-socket split is the runtime/client boundary rule
in code. The state/runtime split lives in `src/app/` (`state.rs` pure data,
`actions.rs` mutations, `input/` key and mouse translation, `runtime.rs` the
live side) with `src/ui/` rendering purely from `&AppState`. Terminal
emulation is the vendored `libghostty-vt` wrapped by `src/ghostty/`; agent
detection is `src/detect/`; persistence and live handoff are `src/persist/`
and `src/server/handoff.rs`.

Fork-added subsystems. All but the tab island are `#[cfg(unix)]`; Windows
builds the upstream feature set, which is what `just windows-lint` guards.

- **Embedded agent watcher.** `src/server/headless/embedded_watcher.rs` starts
  `herdr-agent-watcher` (a git dependency pinned by tag in `Cargo.toml`) inside
  the headless server, and `src/agent_cards/telemetry.rs` ingests its state
  socket. If the standalone `herdr-agent-watcher` *plugin* is enabled, its
  daemon supersedes the embedded one, which exits and by design does not
  restart. `src/cli/watcher.rs` is the `vimeflow watcher` CLI. Bumping the
  watcher means changing the tag in `Cargo.toml`, refreshing `Cargo.lock`, and
  updating the matching `cargoLock.outputHashes` entry in `nix/package.nix`.
- **Agents sidebar cards.** `src/agent_cards/view.rs` is only an adapter: the
  cards themselves (header, task line, model, context/cache/cost, tools,
  traces) are rendered by `herdr_agent_watcher::sidebar::view`, so a change to
  what a card looks like belongs in the watcher repo, not here:
  <https://github.com/winoooops/herdr-agent-watcher> (the plans in
  `docs/vimeflow/plans/` assume a sibling checkout at `~/projects/agent-watcher`).
  What is local is which pane a card belongs to, its workspace, and the palette.
  `src/ui/sidebar.rs` dispatches between cards (`ui.sidebar.agents_view =
  "cards"`, the default) and upstream's legacy rows, and draws agent marks on
  the compact rail. `src/app/input/trace.rs` is `Mode::Agents` keyboard
  navigation with a card zone and a trace zone; the cursor never moves pane
  focus and Enter commits. Its geometry and selectability also come from the
  watcher crate so the two cannot disagree.
- **Title sync.** `src/title_sync/` derives pane titles from the agent's own
  session title: `readers.rs` reads each agent's session store (claude, codex,
  kimi, opencode), `policy.rs` decides whether a rename is allowed (a manual
  rename always wins), `engine.rs` tracks ownership, and `orchestration.rs`
  exposes `title_for_pane`. `src/app/title_sync.rs` gathers the inputs from
  `AppState`.
- **Tab island.** `src/ui/island.rs`, called from `src/ui/tabs.rs`, replaces
  the labeled tab bar with a capsule of tab markers, spring-animated on tab
  change, plus an unread bell and a notification history panel. It is the
  default (`ui.tab_bar_style = "island"`; `"classic"` restores upstream's
  bar). Records, panel state, and animation live in `AppState` as TUI
  presentation state, deliberately not on the API. Design:
  `docs/vimeflow/specs/2026-08-30-dynamic-island-tab-bar-design.md`.
