# Ground Zero Round 1 inventory

Date: 2026-08-27

Issue: [#3](https://github.com/winoooops/vimeflow-terminal/issues/3)

Scope: read-only inventory only; this round removes no files and changes no build or test behavior.

## Executive summary

The inventory baseline is fork `origin/main` at
`550b21a44232f80e3e303259f45812968a496631`, upstream `master` at
`7b675f42af35508eab66ac42fe1598628597a893`, and upstream base tag `v0.8.0`
at commit `346411fa21afd297f5ed3b3fa56f9e3fbf7654b7` (annotated tag object
`857196dee1ce98df53efdd3f437aa2ac8a75b608`).

The repository has 2,404 tracked files occupying 35,139,077 bytes
(33.51 MiB). The complete candidate-removal inventory contains 779 files and
9,696,281 bytes (9.25 MiB, 27.6% of tracked bytes):

| Candidate class | Churn rule | Files | Bytes | MiB |
| --- | --- | ---: | ---: | ---: |
| Removable low churn | 0-2 upstream commits | 76 | 3,299,232 | 3.15 |
| Removable high churn | 3+ upstream commits | 703 | 6,397,049 | 6.10 |
| **All removable candidates** | | **779** | **9,696,281** | **9.25** |

The largest churn hotspots are `docs/next` release staging (70 commits), the
remaining English `docs/next` pages (30), translated `docs/next` pages (21),
the website shell (19), `website/src` (12), and `website/agent-detection`
(10). The latest-release dry-run merge currently has eight ordinary content
conflicts and no modify/delete conflicts.

## Method and definitions

- “Binary-essential” means the path is read by `cargo build --locked`, a Rust
  test target, or the macOS/Linux fork CI build/test jobs. Repository controls
  needed to reproduce those checks are retained with this class.
- “Fork-owned” means Vimeflow policy, documentation, scripts, CI, or additive
  source. Fork ownership is about merge authority, not whether the path began
  upstream.
- “Removable” means outside the compiled/tested fork product surface. Some
  candidates are still wired into upstream `just` recipes or upstream
  workflows; those consumers must be removed or retargeted in the same future
  PR. This report does not make that change.
- Churn is the number of distinct commits returned by
  `git rev-list --count v0.8.0..upstream/master -- <pathspec>`. Sizes are the
  sum of current tracked working-tree file bytes, not allocated filesystem
  blocks. Candidate units below are disjoint.
- Low versus high churn uses a deliberately mechanical boundary: at most two
  upstream commits is low; three or more is high.

`cargo metadata --no-deps --format-version 1` reports one package (`herdr`),
one binary target (`src/main.rs`), `build.rs`, and eleven integration-test
targets. Cargo has no workspace member for the website, Workers, packaging,
or documentation trees.

## Full-tree classification

Every tracked root path is covered below. Split rows name the retained and
candidate portions explicitly.

| Path | Class | Evidence / boundary |
| --- | --- | --- |
| `Cargo.toml`, `Cargo.lock` | Binary-essential, fork-modified | Define and lock the only Cargo package and its Unix watcher/SQLite dependencies. |
| `build.rs` | Binary-essential | Reads and builds the vendored `libghostty-vt` tree. |
| `src/**` | Binary-essential, partly fork-owned | The binary/library source and adjacent unit tests. |
| `tests/**` | Binary-essential, partly fork-owned | Eleven Cargo integration targets plus fixtures. |
| `vendor/**` | Binary-essential | `build.rs` compiles `vendor/libghostty-vt`; Cargo patches `portable-pty` to the vendored path. |
| `assets/sounds/**` | Binary-essential | `src/sound.rs` uses `include_bytes!` for both MP3 files. |
| `docs/next/api/herdr-api.schema.json` | Binary-essential, fork-modified | `src/cli/api.rs` uses `include_str!`; schema tests also compare this file. |
| `skills/herdr/SKILL.md` | Binary-essential | `src/main.rs` uses `include_str!`. |
| `website/latest.json` | Binary-essential test compatibility island | `src/update.rs` includes it in the stock-manifest regression test. Remove only after an explicit test/code retargeting change. |
| `CHANGELOG.md` | Binary/runtime compatibility | `src/release_notes.rs` reads it for local release-note preview behavior retained by the fork. |
| `.config/nextest.toml`, `rust-toolchain.toml`, `clippy.toml`, `justfile` | Binary validation controls | Select toolchain/lints, the scoped graphics retry, and the repository’s local validation entry points. |
| `.gitattributes`, `.gitignore`, `LICENSE` | Fork-owned repository/legal controls | Preserve line-ending/ignore policy and Apache-2.0 licensing. |
| `AGENTS.md`, `CLAUDE.md`, `README.md`, `FORK.md`, `MODIFICATIONS` | Fork-owned governance/framing | Operator PR #2 separately refreshes fork framing; this inventory does not overlap it. |
| `.github/workflows/fork-ci.yml` | Fork-owned | The actual macOS/Linux `cargo build` plus `cargo nextest` gate for `main`. |
| `docs/vimeflow/**` | Fork-owned | Specs, plans, reviews, and this exploration. |
| `docs/next/website/src/content/docs/agents.mdx` | Fork-owned, upstream path | Vimeflow built-in watcher/title/cards user documentation. |
| `docs/next/website/src/data/config-reference.json` | Fork-owned, upstream path | Fork configuration reference and a maintenance-test input. |
| `scripts/preseed_zig_cache.sh` | Fork-owned | Cold-cache build workaround used before local Cargo builds. |
| `src/app/title_sync.rs`, `src/agent_cards.rs`, `src/agent_cards/**`, `src/cli/watcher.rs`, `src/server/headless/embedded_watcher.rs`, `src/title_sync/**` | Fork-owned source | Additive Vimeflow watcher, title-sync, and Agent-card implementation. They remain within the essential `src` tree. |
| `tests/watcher_cli.rs`, `tests/watcher_dependency_smoke.rs` | Fork-owned tests | Additive fork integration/smoke targets. |
| Remaining validation scripts | Binary validation controls | Retain `scripts/__init__.py`, agent-detection/config-reference checks and tests, vendor build/check scripts and tests, Hermes asset test, live-handoff smoke, `windows_check.ps1`, the Windows enhanced-input probe, and Windows smoke-path check. |
| `.agents/**`, `.githooks/**`, upstream `.github/**` except `fork-ci.yml`, `.pi/**`, `.zed/**` | Removable; measured below | Upstream contributor/automation/editor surfaces, not Cargo or fork CI inputs. |
| `CONTRIBUTING.md`, `README.zh-CN.md`, `SPONSORS.md` | Removable; measured below | Upstream contribution, translation, and sponsor surfaces. The root `README.md` is deliberately excluded because PR #2 makes it fork framing. |
| `assets/**` except `assets/sounds/**` | Removable; measured below | Website/social/sponsor media; no Rust reference. Logo removal is conditional on PR #2 landing because current upstream root READMEs reference it. |
| `docs/preview/**`, `docs/versions/**`, remaining `docs/next/**` | Removable; measured below | Upstream release snapshots/staging/translations, outside Cargo and fork CI. |
| `flake.nix`, `flake.lock`, `nix/**` | Removable; measured below | Nix distribution surface; not used by Cargo or fork CI. |
| `packaging/**` | Removable; measured below | Windows ConPTY distribution metadata/assets. |
| Release, packaging, capture, and website scripts listed below | Removable; measured below | Not part of Cargo/fork CI; several are coupled to upstream CI/`just test` and must move together. |
| `website/**` except `website/latest.json` | Removable; measured below | Astro website, installers, documentation snapshots, and marketing assets. |
| `workers/**` | Removable; measured below | Plugin marketplace Worker; currently reached only by `just test`/`just check`. |

### Build and test graph details

The direct compile-time file edges are small and explicit:

- `build.rs` reads `vendor/libghostty-vt.vendor.json`, `build.zig`,
  `build.zig.zon`, headers/package metadata, Zig sources, and `VERSION`.
- `[patch.crates-io]` points `portable-pty` to `vendor/portable-pty`.
- `src/sound.rs`, `src/main.rs`, and `src/cli/api.rs` embed the two sound
  files, the Herdr skill, and the API schema respectively.
- Rust tests consume `tests/fixtures/**`; `src/update.rs` has the single
  `website/latest.json` test edge.
- Fork CI runs only `cargo build --locked` and `cargo nextest run --locked` on
  macOS and Linux.

The broader `just test` recipe additionally runs maintenance Python tests,
the two Bun integration-asset tests, and `workers/plugin-marketplace` tests.
It currently names release/preview/translation and Windows-package test
modules. Therefore future deletion of those modules or the Worker must update
`justfile` in the same PR; deleting files alone would break the documented
local test command even though Cargo and fork CI would remain green. Upstream
`.github/workflows/ci.yml` likewise consumes `scripts/conventional_commits.py`
and Windows packaging scripts, so those are coupled to workflow retirement.

## Removable-path measurements

“Delete” below means a future Round 2 change, never this inventory. “Keep
excluded” means retain upstream bytes but prevent the surface from running or
shipping. “Conditional delete” names a dependency that must be removed in the
same PR.

| Removable path/unit | Class | Files | Bytes | Upstream commits | Recommended treatment |
| --- | --- | ---: | ---: | ---: | --- |
| `.agents/**` | High | 4 | 18,957 | 5 | Delete + registry/guard. |
| `.githooks/**` | Low | 2 | 309 | 1 | Delete + registry/guard; remove `install-hooks` recipe. |
| `.github/DISCUSSION_TEMPLATE/**` | Low | 2 | 942 | 0 | Delete + registry/guard. |
| `.github/ISSUE_TEMPLATE/**` | High | 3 | 5,303 | 3 | Delete + registry/guard. |
| `.github/FUNDING.yml` | Low | 1 | 21 | 1 | Delete + registry/guard. |
| `.github/MAINTAINERS` | High | 1 | 177 | 3 | Delete + registry/guard after fork policy no longer reads it. |
| `.github/dependabot.yml` | Low | 1 | 402 | 0 | Delete + registry/guard unless fork-owned dependency updates are desired. |
| `.pi/**` | Low | 6 | 45,097 | 1 | Delete + registry/guard. |
| `.zed/**` | Low | 1 | 1,252 | 0 | Delete + registry/guard. |
| `CONTRIBUTING.md` | High | 1 | 8,284 | 4 | Delete + registry/guard after fork contribution policy lives in fork docs. |
| `README.zh-CN.md` | High | 1 | 4,540 | 5 | Delete + registry/guard after PR #2 lands. |
| `SPONSORS.md` | Low | 1 | 2,368 | 2 | Delete + registry/guard. |
| `assets/{og-card.png,screenshot.png}` | Low | 2 | 983,141 | 0 | Delete + registry/guard. |
| `assets/sponsors/**` | Low | 2 | 15,472 | 0 | Delete + registry/guard. |
| `assets/{logo.png,logo.svg}` | Low | 2 | 122,811 | 0 | Conditional delete after PR #2 removes the upstream README references. |
| `docs/preview/**` | High | 62 | 677,437 | 4 | Delete + registry/guard; generated release snapshot. |
| `docs/versions/**` | High | 422 | 3,509,418 | 8 | Delete + registry/guard; immutable upstream release snapshots dominate savings. |
| `docs/next/{CHANGELOG.md,README.md,README.zh-CN.md,product-announcement.json}` | High | 4 | 110,327 | 70 | Keep excluded first; delete only with release recipes/workflows. |
| `docs/next/website/src/content/docs/{ja,zh-cn}/**` | High | 40 | 451,992 | 21 | Keep excluded first; delete with translation checks/recipes. |
| Remaining upstream English `docs/next` pages (excluding fork `agents.mdx`) | High | 19 | 185,809 | 30 | Keep excluded until the fork-owned user-doc home is decided, then register/delete. |
| `flake.nix`, `flake.lock`, `nix/**` | Low | 3 | 6,206 | 0 | Delete + registry/guard with Nix workflow. |
| `packaging/windows/**` | Low | 3 | 26,150 | 0 | Conditional delete with Windows packaging scripts/workflow jobs. |
| `workers/plugin-marketplace/**` | Low | 5 | 22,404 | 2 | Conditional delete with `just` marketplace test recipes. |
| `website/agent-detection/**` | High | 20 | 30,762 | 10 | Keep excluded first; retained manifest validation currently has a `--require-website` release mode. |
| `website/assets/**` | Low | 33 | 1,962,712 | 2 | Delete + registry/guard. |
| `website/css/**` | High | 3 | 137,923 | 4 | Keep excluded with website source until wholesale website deletion. |
| `website/scripts/**` | High | 8 | 44,862 | 8 | Keep excluded with website source until release/website recipes are retired. |
| `website/src/**` | High | 89 | 837,560 | 12 | Keep excluded first; delete the website atomically, not piecemeal. |
| Website root shell excluding `latest.json` | High | 13 | 255,440 | 19 | Keep excluded first; includes Astro/Bun/installers and `preview.json`. |
| `.github/workflows/build-artifacts-manual.yml` | Low | 1 | 9,221 | 0 | Delete + registry/guard. |
| `.github/workflows/ci.yml` | Low | 1 | 9,669 | 0 | Conditional delete with its conventional-title and Windows packaging consumers. |
| `.github/workflows/issue-gate.yml` | Low | 1 | 10,929 | 2 | Delete immediately in Round 2; it already fails in this fork without its secret. |
| `.github/workflows/label-next-release-issues.yml` | Low | 1 | 4,818 | 0 | Delete + registry/guard. |
| `.github/workflows/nix.yml` | Low | 1 | 1,403 | 0 | Delete with Nix files. |
| `.github/workflows/pr-gate.yml` | High | 1 | 10,451 | 3 | Delete + registry/guard; its job is already repository-gated off here. |
| `.github/workflows/preview.yml` | Low | 1 | 21,363 | 1 | Delete + registry/guard; token/deploy-hook dependent. |
| `.github/workflows/release.yml` | Low | 1 | 16,793 | 2 | Delete + registry/guard until the fork designs its own release pipeline. |
| `.github/workflows/website.yml` | Low | 1 | 1,362 | 1 | Delete with the website surface. |
| Release/website policy scripts | High | 8 | 81,527 | 4 | Keep excluded first; delete with `just` release/test recipes. |
| Windows packaging scripts | High | 4 | 26,280 | 5 | Conditional delete with upstream CI/build-artifact jobs and packaging metadata. |
| Capture/diagnostic utility scripts | Low | 4 | 34,387 | 0 | Delete + registry/guard if the fork does not adopt these maintainer tools. |

The grouped script units above are exact:

- Release/website policy: `changelog.py` and its test,
  `conventional_commits.py`, `docs_translation_parity.py` and its test,
  `preview.py` and its test, and `seed_navigator_demo.sh`.
- Windows packaging: `package_windows_conpty.ps1`,
  `package_windows_conpty.py`, its Python test, and
  `windows_install_conpty_package_test.ps1`.
- Capture/diagnostic utilities: `capture_agent_screen.py`,
  `capture_key_matrix.py`, `capture_keys.py`, and
  `verify_suspicious_keys.py`.
- Website root shell: `.gitignore`, `README.md`, `_headers`, `_redirects`,
  `agent-guide.md`, `astro.config.mjs`, `bun.lock`, `index.html`, `install.ps1`,
  `install.sh`, `package.json`, `preview.json`, and `robots.txt` under
  `website/`. `website/latest.json` is not in this unit.

PR #2 is intentionally outside this branch. The logo and translated-root
README recommendations assume its fork-framing changes land; otherwise those
references must remain until an equivalent framing change does.

## Workflow inventory

There are ten tracked workflows: one fork-owned gate and nine inherited
upstream workflows. GitHub reports all ten as active. “Upstream-only” cannot
be inferred from branch names: `master` still exists in this fork as the
upstream mirror, and tag/manual/event triggers still operate in the fork.

| Workflow | Trigger in this fork | Secrets / side effects | Current status |
| --- | --- | --- | --- |
| `fork-ci.yml` | PRs to `main`; pushes to `main` | None; read-only build/test | Fork-owned intended gate. |
| `ci.yml` | Every PR regardless of base; pushes to `master`/`windows` except website-only pushes | None; runs conventional-title, Linux/macOS/Windows checks and Windows packaging | **Fires on fork PRs.** PR #2 produced CI jobs. Not neutralized. |
| `issue-gate.yml` | Every newly opened issue | Needs `KANGAL_GITHUB_TOKEN`; can comment/close issues | **Fires and fails here.** Issue #3 run `33081394576` failed with `Input required and not supplied: github-token`. Not neutralized. |
| `pr-gate.yml` | `pull_request_target` opened/edited/reopened/synchronized | Would need `KANGAL_GITHUB_TOKEN` and write to issues/PRs | Workflow fires, but its only job is neutralized by `github.repository == 'herdrdev/herdr' || 'ogulcancelik/herdr'`; PR #2 shows it skipped. |
| `nix.yml` | Any PR changing Cargo/assets/vendor/Nix/skills paths; matching pushes to `master` | None | Can fire in the fork; not neutralized. |
| `website.yml` | Any PR changing website/docs snapshot paths; matching pushes to `master` | None | Can fire in the fork; not neutralized. |
| `label-next-release-issues.yml` | Pushes to mirror branch `master` | Needs `KANGAL_GITHUB_TOKEN`; can close issues | Fires when the fork updates its mirror; not neutralized. |
| `build-artifacts-manual.yml` | Manual dispatch | No repository secret | Can be invoked in the fork; upstream packaging surface. |
| `preview.yml` | Manual dispatch | Needs `KANGAL_GITHUB_TOKEN` and `CLOUDFLARE_PAGES_DEPLOY_HOOK`; creates prereleases, commits docs, deploys | Enabled but unusable as intended without upstream secrets; not repository-gated. |
| `release.yml` | Any pushed `v*` tag | Needs `KANGAL_GITHUB_TOKEN`, `RELEASE_DEPLOY_KEY`, and `CLOUDFLARE_PAGES_DEPLOY_HOOK`; publishes assets/docs | Would fire on fork tags; not repository-gated. |

The safe Round 2 order is to remove harmful event automation first
(`issue-gate`, release/preview/label workflows), preserve `fork-ci.yml`, then
retire upstream `ci.yml` only together with any validation it uniquely provides
that the fork still wants. Merely lacking a secret is not neutralization: it
turns an inherited workflow into a red run, as issue #3 demonstrates.

## Latest-release dry-run merge baseline

`v0.8.2` is the latest upstream release tag. It resolves to commit
`9eb521456ac0d19d3ab3d9d7cea3cca10baa8a4c` (annotated tag object
`34ba52cc6ff3b723e6fc0130485ec24582dbe205`). The probe used a temporary
worktree and branch based on the unchanged `origin/main`:

```text
git fetch upstream --tags
git worktree add -b probe/issue-3-v0.8.2 <temporary-directory> origin/main
git merge --no-commit --no-ff v0.8.2
# record status and unmerged paths
git merge --abort
git worktree remove <temporary-directory>
git branch -D probe/issue-3-v0.8.2
```

The merge exited 1 with exactly eight content conflicts:

```text
Cargo.lock
docs/next/website/src/data/config-reference.json
src/app/mod.rs
src/app/state.rs
src/config/io.rs
src/config/model.rs
src/server/headless.rs
src/ui/sidebar.rs
```

`git diff --name-only --diff-filter=U` and `git ls-files -u` showed no other
unmerged path. There are **zero modify/delete conflicts today**, because no
candidate has been deleted. `git merge --abort` restored a clean probe; the
temporary worktree and branch were removed.

This baseline is useful but not a forecast: registered deletions will turn
later upstream modifications under those paths into expected modify/delete
conflicts. That is why deletion needs an explicit policy and guard rather than
an informal list in a commit message.

## Recommendation: hybrid, with explicit deletion policy

Use the issue’s hybrid candidate:

1. **Binary-essential:** keep and merge normally. Do not put these paths in a
   deletion registry. Fork-modified upstream files continue using the existing
   section-4(b) notice and upstream-edit registry.
2. **Fork-owned:** keep; `main` is authoritative. New fork-only paths cannot
   conflict with upstream unless upstream later creates the same path, which
   must be reviewed as a namespace collision.
3. **Removable low churn:** delete with a machine-readable path/prefix registry
   and a CI reappearance guard. Low churn yields 3.15 MiB with limited expected
   merge friction.
4. **Removable high churn:** keep-but-exclude by default in the first cleanup
   pass. Delete high-value exceptions atomically: generated `docs/preview` and
   `docs/versions` snapshots, and harmful upstream automation. Move the
   remaining website/release/docs sources to deletion only after fork-owned
   docs/release choices and their `justfile` consumers are settled.

This avoids two bad extremes: carrying active upstream automation because it
is churny, or deleting frequently changed source-shaped trees before the fork
has replaced the few local contracts that still point at them.

### Modify/delete merge policy

- `master` remains a pristine fast-forward mirror. Deletions exist only on
  product branch `main`.
- Before every upstream release merge, fetch tags and review the upstream diff
  for every registered removed path/prefix.
- When upstream **modifies** a registered removed file, resolve the
  modify/delete conflict in favor of deletion on `main` only after deciding
  whether its behavior must be ported into a retained binary-essential or
  fork-owned path. Record any port as a normal fork modification.
- When upstream **adds** a file under a registered removed prefix, the guard
  fails. Delete it if it belongs to the retired surface. If it introduces a
  real binary/test dependency, remove or narrow the registry entry in a
  dedicated reviewed PR; never silently force-delete a new dependency.
- For keep-but-exclude paths, do not fork-edit them. Accept the upstream side
  during merges and keep them absent from fork workflows, packaging, and
  documented validation. This trades disk savings for conflict avoidance.
- The guard should compare tracked paths to exact registry paths/prefixes in
  fork CI. It should not inspect filesystem ignores or depend on local Git
  configuration. A merge that resurrects a retired path must fail before it
  reaches `main`.
- Re-run the same `--no-commit --no-ff` release-tag probe after each cleanup
  PR. The expected baseline becomes the eight existing conflicts plus reviewed
  modify/delete conflicts for upstream changes under newly registered paths.

## Reproducible evidence commands

These are the commands used for the inventory/probe, with pathspec loops used
to produce the table rows:

```text
gh repo set-default --view
# winoooops/vimeflow-terminal

git fetch upstream --tags
gh release list --repo herdrdev/herdr --limit 10
git rev-parse origin/main upstream/master v0.8.0 v0.8.0^{commit}
git tag --sort=-version:refname
git ls-files | wc -l
git ls-files -z | xargs -0 stat -f '%z' | awk '{sum += $1} END {print sum}'
cargo metadata --no-deps --format-version 1
rg -n 'include_(bytes|str)!|vendor/|docs/next|website/latest|workers|packaging' \
  Cargo.toml build.rs src tests scripts justfile .github/workflows

git rev-list --count v0.8.0..upstream/master -- <candidate-pathspecs>
git ls-files -z -- <candidate-pathspecs> | xargs -0 stat -f '%z'

gh workflow list --all
gh run view 33081394576 --log-failed
gh pr checks 2
```

All size and churn results describe the Round 1 baseline above. This document
does not authorize or perform Round 2 removals.
