<!-- Modified from herdr by the vimeflow project — see FORK.md -->

# vimeflow-terminal

**an opinionated downstream fork of [herdr](https://github.com/herdrdev/herdr).**

<p align="center">
  <a href="https://github.com/herdrdev/herdr">upstream herdr</a> ·
  <a href="https://herdr.dev/docs/">herdr docs</a> ·
  <a href="FORK.md">what this fork changes</a> ·
  <a href="#try-it">try it</a>
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-666666?labelColor=333333" alt="Apache 2.0 license" /></a>
</p>

---

## try herdr first

**If you have not used [herdr](https://herdr.dev), start there.** Herdr is the
real thing: an agent multiplexer that lives in your terminal — every agent's
state at a glance, detach and reattach from anywhere, a pure socket API agents
can drive themselves, keyboard and mouse both first-class, one Rust binary.

```bash
curl -fsSL https://herdr.dev/install.sh | sh
```

Herdr is installable, documented, released on a schedule, and supported. This
fork is none of those things yet. Almost everything you would want, herdr
already does — and it does it better, because upstream ships it.

Come back here only if you have used herdr and specifically want the
opinionated layer described below.

## what this fork is

Vimeflow was an Electron desktop app for coding agents. In August 2026 it
pivoted to terminal-native, rebuilt as a **tracking fork of herdr v0.8.0**.

The split of labor is deliberate:

- **herdr supplies the engine** — PTY ownership, VT emulation via vendored
  libghostty-vt, the workspace → tab → pane model, git and worktree state,
  agent detection, notifications, the plugin system, the socket API.
- **vimeflow supplies an opinionated layer on top** — agent observability as
  built-in chrome rather than an optional plugin, and a set of workflow
  features that assume you are running several coding agents at once.

Shortest description: **the opinionated herdr**. Upstream stays general; this
fork makes choices for you. If a choice here turns out to be right for
everybody, it belongs upstream, not in a fork.

This is a *tracking* fork, not a hard fork. Upstream releases are merged in on
a per-release cadence, every edit to an upstream file is logged, and the goal
is to stay mergeable — not to drift.

## what's here today

Everything herdr v0.8.0 does, plus:

- **built-in agent watcher** — coding-agent observability (transcript
  watchers, lifecycle, metrics, notifications) is compiled into the binary and
  runs with the server. No plugin to install, link, or keep in sync. CLI under
  `vimeflow watcher`.
- **automatic pane titles** — pane labels follow the agent's session title as
  it works. A manual rename always wins and is never overwritten.
- **Agents sidebar cards** — the sidebar's Agents section renders agent cards
  (lifecycle plus context, cache, cost, model, tools, traces) instead of a
  token-row list. The cards are drawn by the agent watcher itself, so there is
  one implementation of what a card looks like, not two. Configurable live:

  ```toml
  [ui]
  agents_view = "cards"    # or "legacy" for herdr's row list
  agents_hide_idle = false
  ```

- **keyboard agent navigation** — `prefix+a` moves focus into the Agents
  sidebar. Two zones: `j`/`k` walk the cards and `l` descends into the
  selected card's tool-call traces, where `o` opens a detail panel showing
  status, duration, timestamp and the full retained arguments. Moving the card
  cursor deliberately does *not* change pane focus — only Enter commits — so
  walking a long list never throws the main view around. Mouse works
  throughout: click a trace row to select it, click again to open it.

- **no phone-home** — self-update, hosted manifest fetches, and product
  announcements are deliberately disabled. This fork will never install stock
  herdr over itself.

The native features above are **Unix-only** (macOS and Linux). Windows builds
the upstream feature set.

## what's coming

No dates. Roughly in order:

- **pane cards and the worktree flow** — card-style pane headers with agent
  glyph, state, and worktree badge; and *"new agent pane in a fresh worktree"*
  collapsed into a single action that splits the current tab instead of
  spawning a new workspace.
- **agent rows in the navigator** — per-agent state chips and latest task
  message under each workspace/tab entry; click to focus the pane.
- **local hunk view** — a TUI diff pane over an in-process git engine:
  worktree-scoped file list, hunk-level rendering and navigation, read-only
  first.

Still partly deferred: the branding rename covers the executable and the
config and state directories, but **not** the `HERDR_*` environment variables
or the `herdr.sock` / `herdr-client.sock` filenames. Those are the integration
protocol — `herdr-agent-watcher` alone reads nine of those variables — so
renaming them would break plugins for no user-visible gain. See the deferred
branding surface in [`FORK.md`](FORK.md).

## try it

There are no prebuilt binaries, no installer, and no release channel. Build
from source.

**Prerequisites.** Rust 1.96.1 (pinned in `rust-toolchain.toml`, so `rustup`
picks it up automatically) and **Zig 0.15.2** to compile the vendored
libghostty-vt. A *newer* Zig on `PATH` will fail — the version is exact. On a
cold Zig cache, run `scripts/preseed_zig_cache.sh` first, or the build dies
fetching a dependency tarball.

```bash
git clone https://github.com/winoooops/vimeflow-terminal
cd vimeflow-terminal
scripts/preseed_zig_cache.sh      # only needed on a cold Zig cache
cargo build --release
./target/release/vimeflow
```

The executable is `vimeflow`, not `herdr`. Put it on your `PATH` if you want:

```bash
ln -s "$PWD/target/release/vimeflow" ~/.local/bin/vimeflow
```

### coexisting with herdr

vimeflow keeps its own directories, so an installed herdr is untouched:

| | vimeflow | upstream herdr |
| --- | --- | --- |
| executable | `vimeflow` | `herdr` |
| config | `~/.config/vimeflow/` | `~/.config/herdr/` |
| state | `~/.local/state/vimeflow/` | `~/.local/state/herdr/` |

vimeflow starts from a **clean config**; nothing is inherited from an installed
herdr. Copy `~/.config/herdr/config.toml` across by hand if you want your
settings.

The two can run **at the same time**: a server exports its own socket path into
the panes it spawns, so each side's children resolve back to the side that
started them. `HERDR_*` variable names and the `herdr.sock` filenames are shared
on purpose so plugins keep working, which has one consequence — launching
vimeflow *from inside a herdr pane* inherits that pane's `HERDR_SOCKET_PATH` and
attaches to herdr. Start it from a terminal outside any session, or clear the
variables:

```bash
env -u HERDR_ENV -u HERDR_SOCKET_PATH -u HERDR_CLIENT_SOCKET_PATH vimeflow
```

Check which one you are on with `vimeflow status server` — the socket path names
the side that owns it.

One thing genuinely cannot be shared: the **Claude metrics bridge** is a single
hook in the global `~/.claude/settings.json`, so only one install can own it.
Whichever ran `watcher claude-bridge enable` last gets Claude's context, cache
and cost numbers; the other shows `—` for them.

### configuration

```bash
vimeflow --default-config > ~/.config/vimeflow/config.toml
```

Every setting is listed and **commented out**, showing its default. Uncomment
only what you want to change. Fork-only settings are marked as such and
gathered at the end of the file.

### a gotcha worth knowing

If your cards all read `— no telemetry`, the standalone agent-watcher *plugin*
is enabled and is fighting the built-in one. The plugin launches its own daemon
at server startup, which supersedes the embedded watcher; the embedded one then
exits and, by design, does not restart. Disable the plugin:

```bash
vimeflow plugin disable herdr-agent-watcher
vimeflow server stop && vimeflow      # restart to pick it up
```

The plugin's own `open-sidebar` keybinding keeps working either way — vimeflow
routes it to the built-in Agents sidebar.

### running a server without attaching

`vimeflow` starts a server and attaches to it. To restart a server without
losing your terminal, stop it first — this kills every pane in the session:

```bash
vimeflow server stop
vimeflow
```

### tests

```bash
just test     # unit tests + maintenance checks
just check    # the full gate: lint, tests, Windows-target clippy
```

Without `just`, the fork's CI command is:

```bash
cargo nextest run --locked -E 'not binary(live_handoff)'   # macOS
cargo nextest run --locked                                  # Linux
```

`live_handoff` is excluded on macOS, matching upstream. Integration binaries
that spawn a real server run one at a time (see `.config/nextest.toml`); they
compete for descriptors and fs watches otherwise and time out.

## how this fork tracks herdr

- `main` is the product branch and carries all Vimeflow work.
- `master` is a fast-forward-only mirror of `upstream/master`. Nothing is
  committed there.
- Upstream releases are merged into `main` through review branches, once per
  release rather than once per commit.
- Every modified upstream file carries an in-file change notice and a row in
  the [`FORK.md`](FORK.md) registry; non-commentable files are listed in
  [`MODIFICATIONS`](MODIFICATIONS).

[`FORK.md`](FORK.md) is the full record: fork base commit, the path-by-path
upstream-edit registry, the merge procedure, and the known-baseline test
behavior. [`AGENTS.md`](AGENTS.md) is the guidance for AI agents working in
this repository.

## license and attribution

This fork is licensed under the [Apache License 2.0](LICENSE), the same license
as herdr, and preserves upstream's LICENSE unchanged.

Herdr is copyright the herdr contributors and is created and maintained by
[@ogulcancelik](https://github.com/ogulcancelik). This project exists only
because that work is open source, and upstream deserves the credit and the
support — **if this fork is useful to you, [sponsor
herdr](https://github.com/sponsors/ogulcancelik)**, not this fork.

Vendored dependencies carry their own licenses: `libghostty-vt` (MIT, ©
Mitchell Hashimoto) and `portable-pty` (MIT, © Wez Furlong). The vendored
`libghostty-vt/pkg` tree includes additional third-party material under other
terms; any future binary distribution of this fork must ship a license
inventory covering it.

"herdr" is the upstream project's name, used here only to describe this fork's
origin. This project claims no rights to it.
