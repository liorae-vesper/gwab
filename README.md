# Rust project template

A small, batteries-included starting point for Rust projects. It gives a new
repository a reproducible toolchain, one task interface for every check, Git
hooks that enforce formatting and Conventional Commits, dependency policy and
vulnerability auditing, a shared compiler cache, and CI that runs the same
gates — without adding a task runner, a build system, or any architectural
opinion about what the crate should do.

The crate itself is deliberately trivial (`crwst::greeting()`). This repository
is a development-tooling template, not an application skeleton.

## What is in the box

| Tool | Role | Why it is here |
| --- | --- | --- |
| [mise](https://mise.jdx.dev) | Tool versions, environment, task runner | Pins every tool and gives one command interface (`mise run <task>`). It replaces a `just`/`make` file, so there is one task definition rather than two. |
| [mr-boxington](https://mr-boxington.jdx.dev) (`mbx`) | Shared compiler cache | Reuses Cargo compilations across checkouts, worktrees and CI. Enabled through mise, so plain `cargo` is cached with no extra wrapper to remember. |
| [hk](https://hk.jdx.dev) | Git hooks | Runs the mise tasks as hooks. Configuring hooks here rather than in a shell script keeps hook and CI behaviour in sync. |
| [Cocogitto](https://docs.cocogitto.io) (`cog`) | Conventional Commits validation | Checks that every commit message follows the Conventional Commits spec. It is the only tool here that inspects history instead of code, and it keeps a shared repository's log parseable and readable. |
| [cargo-deny](https://embarkstudios.github.io/cargo-deny/) | Dependency **policy** | Bans `*` requirements, duplicate crates and unknown registries, and enforces a licence allow-list. |
| [cargo-audit](https://rustsec.org) | Dependency **vulnerabilities** | Checks `Cargo.lock` against the RustSec advisory database. |
| [cargo-nextest](https://nexte.st) | Test runner | Faster, per-test parallel test runs with profiles for local and CI use. |
| [cargo-machete](https://github.com/bnjbvr/cargo-machete) | Unused dependencies | Finds dependencies that are declared but never used. |
| [bacon](https://dystroy.org/bacon) | Continuous feedback | Watches the workspace and re-runs check, clippy, tests or docs as you edit. |
| rustfmt | Formatting | Shipped as a Rust toolchain component; run through `cargo fmt`. |
| Clippy | Lints | Shipped as a Rust toolchain component; run through `cargo clippy`. |

The lints the whole project shares live in `Cargo.toml` under `[lints]`, so
bacon, Clippy locally and CI all agree on what "lint clean" means. `clippy.toml`
holds their *configuration*, as opposed to their levels.

`unwrap`, `expect` and `panic!` are denied, so a failure is returned as an error
instead of aborting the process. Tests are the exception, and the exemption is
declared once in `clippy.toml` rather than repeated as `#[allow(...)]`
attributes in every test file:

```toml
allow-unwrap-in-tests = true
allow-expect-in-tests = true
allow-panic-in-tests = true
```

That covers unit tests in `#[cfg(test)]` modules and integration tests under
`tests/` alike, so a new test file needs no attribute of its own. It affects only
the three restriction lints named after them: a test that unwraps something the
compiler can see through still trips `unnecessary_literal_unwrap`, because that
lint is about the code being pointless, not about panicking.

## Start a new project from the template

```sh
# Point this at wherever the template lives: a Git URL, or a local path.
TEMPLATE_URL=<the-url-or-path-of-this-template>

git clone "$TEMPLATE_URL" my-project
cd my-project
# Start a fresh history: the template's commits are not yours.
rm -rf .git && git init -b main
```

Then rename the crate. `crwst` appears in these places:

| File | What to change |
| --- | --- |
| `Cargo.toml` | `[package] name` (and `description`) |
| `src/lib.rs` | the doctest and the greeting text |
| `src/main.rs` | the `crwst::` path |
| `tests/smoke.rs` | the `crwst::` path |
| `README.md` | this file |

`Cargo.lock` updates itself on the next `cargo` command. Nothing else refers to
the crate name: the tool configuration is name-agnostic.

## Setup

### 1. Install mise

```sh
curl https://mise.run | sh
```

Other installation methods (Homebrew, apt, cargo) are on
<https://mise.jdx.dev/installing-mise.html>. Then make mise available in your
shell — either activate it, which also picks up tool environment variables:

```sh
# bash
echo 'eval "$(mise activate bash)"' >> ~/.bashrc
# zsh
echo 'eval "$(mise activate zsh)"' >> ~/.zshrc
```

…or skip activation and use mise's shims, which are enough for `mise run` and
for the Git hooks.

Run `mise trust` once per clone. Until then mise prints `mise.toml is not
trusted`: `mise.toml` carries tool options (`mr_boxington`, `components`,
`locked`), so mise cannot treat it as an inert file. The commands that execute
project code — `mise run`, `mise install`, `mise exec`, `mise watch`, and the Git
hooks, which go through `mise x` — trust their active config automatically, and
in CI mise trusts every config, so the warning never blocks a build. Other
commands, such as `mise ls`, may skip the project config until you trust it.

**mise 2026.9.2 or newer is required.** The `mr_boxington` option on the `rust`
tool uses native mise integration added in that release; on an older mise the
Cargo cache is simply not active and `mise install` reports an unknown tool
option.

### 2. Install the pinned tools

```sh
mise install
```

This resolves every version in `mise.lock` — Rust, `mbx`, `hk`, `cog`, and the
five Cargo subcommands. The first run compiles the Cargo subcommands from
source, which takes a few minutes; later runs and CI reuse them from mise's
cache.

### 3. Install the Git hooks

```sh
mise run setup
```

This validates `hk.pkl` and installs the `pre-commit` and `commit-msg` hooks
with `hk install --mise`, so the hooks run through `mise x` and work even in a
shell where mise is not activated (mise must be on `PATH`). On Git 2.54+ hk
writes them into `.git/config`; on older Git it falls back to script shims.
`hk install` is local to the repository — nothing global changes.

### Which tools come from where

| Source | Tools |
| --- | --- |
| Pinned in `mise.toml`, locked in `mise.lock` | Rust toolchain, `mbx`, `hk`, `cog`, `cargo-deny`, `cargo-audit`, `cargo-nextest`, `cargo-machete`, `bacon` |
| Rust toolchain components (from `rust-toolchain.toml`, installed by mise) | `clippy`, `rustfmt` |
| Supplied by Cargo itself | `cargo`, `rustc`, `cargo fmt`, `cargo clippy`, `cargo test`, `cargo doc` |
| Expected on the machine | `git`. Nothing else: no `just`, no `make`, no globally installed Cargo tools. |

`rust-toolchain.toml` exists so that a contributor who uses plain `rustup`
instead of mise still gets the right compiler and components. It pins the same
version as `mise.toml`; **bump both together** (see below).

## Tasks

`mise run <task>` is the only interface. Cargo commands appear in `mise.toml`
and nowhere else, so local runs, hooks and CI cannot drift apart.

| Task | What it does | Runs in |
| --- | --- | --- |
| `fmt` | `cargo fmt --all` | you |
| `fmt-check` | Checks formatting without writing | `check`, hooks, CI |
| `clippy` | Clippy over the workspace, all targets and features, `-D warnings` | `check`, hooks, CI |
| `test` | Nextest, then the doctests only Cargo can run | `check`, hooks, CI |
| `unused` | `cargo machete --with-metadata` | `check`, hooks, CI |
| `deny` | Dependency policy: wildcards, duplicates, licences, sources | `audit`, CI |
| `advisories` | RustSec advisories (`cargo audit`) | `audit`, CI |
| `audit` | `deny` + `advisories` | you, CI |
| `check` | `fmt-check` + `clippy` + `test` + `unused` — the fast local gates | hooks, CI |
| `ci` | `check` + `audit` — everything CI requires | you |
| `commits` | `cog check`: every commit message is Conventional Commits | CI |
| `bacon` | Runs bacon in the foreground | you |
| `setup` | Validates `hk.pkl` and installs the Git hooks | once per clone |

Dependencies are declared with `depends`, so a shared prerequisite runs once even
when several tasks need it, and a failing dependency stops what depends on it.
`mise run ci` is therefore a real end-to-end gate, not a list of repeated work.

Typical use:

```sh
mise run check          # before committing; this is what the hook runs
mise run fmt            # reformat instead of only being told it is wrong
mise run ci             # everything CI runs, locally
mise run bacon          # start the watch loop in another terminal
mise tasks ls           # list tasks
mise run --dry-run ci   # print every command in execution order
```

## Git hooks and Conventional Commits

`hk.pkl` defines three hooks, all of which call mise tasks:

| Hook | Step | Purpose |
| --- | --- | --- |
| `commit-msg` | `cog verify --file {{commit_msg_file}}` | The message must be a Conventional Commit. |
| `pre-commit` | `mise run check` | Formatting, lints, tests and unused dependencies. |
| `check` | `mise run check` | Not a Git event: it is what `hk check` and `hk check --all` run on demand. |

Messages look like `feat(parser): support quoted fields`, `fix: reject empty
input`, `docs: explain the licence split`. `cog verify` rejects anything else.

Two deliberate choices:

* **`pre-commit` never modifies files.** It uses the `check` form of each gate,
  and `stash = "none"`, so a commit cannot silently rewrite your working tree or
  touch the index. Run `mise run fmt` when formatting fails.
* **There is no `pre-push` hook.** It would re-run exactly what `pre-commit`
  already ran. The expensive checks that might have justified one — the RustSec
  advisory download in `mise run advisories` — are CI's job instead.

Bypass a hook for one command when you need to, and note that CI still checks the
result:

```sh
HK=0 git commit -m "wip: needs work"     # skip hk for this commit
git commit --no-verify                   # skip Git hooks entirely
```

Cocogitto owns commit-message validation, and nothing else:

* `cog verify --file <msg>` checks a single message. The `commit-msg` hook runs
  this, so a malformed message never becomes a commit.
* `mise run commits` (`cog check`) checks a whole history. This is what CI runs,
  because `HK=0` can skip the hook.

`cog.toml` holds the two settings that both of them need —
`ignore_merge_commits` and `ignore_fixup_commits` — so `cog check` behaves like
the hook without flags being repeated on every command line. A merge commit or
a `fixup!` message therefore passes, while anything else must be a Conventional
Commit.

## Dependency auditing

Three tools, three questions. They are not interchangeable, so they are separate
tasks:

| Question | Tool | Task |
| --- | --- | --- |
| Is a dependency vulnerable? | cargo-audit (RustSec) | `mise run advisories` |
| May we depend on this crate and its licence? | cargo-deny | `mise run deny` |
| Do we still use everything we declare? | cargo-machete | `mise run unused` |

`mise run deny` checks `bans`, `licences` and `sources` **but not `advisories`**,
on purpose: a vulnerable crate and a disallowed licence need different
responses, and an advisory should not be hidden behind a licence failure.
cargo-audit owns advisories. (cargo-deny *can* check them —
`cargo deny check advisories` — and the `[advisories]` section of `deny.toml` is
where reviewed, accepted risks go.)

`deny.toml` deliberately:

* allows a permissive licence set plus MPL-2.0, and fails on anything else;
* sets `wildcards = "deny"`, so `*` version requirements never enter the tree;
* sets `unknown-registry = "deny"` and `unknown-git = "deny"`, so a dependency
  from somewhere other than crates.io is a visible, reviewed edit;
* keeps `multiple-versions = "warn"`, because duplicates are usually transitive
  and often cannot be resolved by the author;
* sets `unused-allowed-license = "allow"`, because an allow-list always contains
  licences a given dependency tree does not use.

Lint levels live in `deny.toml` rather than on the task's command line, so there
is one place to read the policy. Change a level there to tighten or relax it.

## Build caching with mr-boxington

`mise.toml` enables the `mr_boxington` option on the `rust` tool:

```toml
rust = { version = "1.99.0", profile = "minimal", components = ["clippy", "rustfmt"], mr_boxington = true }
mr-boxington = "1.23.0"
```

Both entries are required. With them, mise runs `cargo` through `mbx` inside
`mise run` tasks and `mise exec`, so every task here is cached without naming
`mbx` anywhere. `mbx` keeps the real build output in its own store and leaves
`target` in the workspace as a symlink to it — which is why `.gitignore` contains
`/target` **without** a trailing slash: git treats the symlink as a file, and
`/target/` would not match it.

Useful commands (`mise exec -- mbx <command>`):

```sh
mbx doctor        # check the installation and cache access
mbx stats         # lifetime savings and storage shared across workspaces
mbx cache stats   # what the store holds
mbx gc --dry-run  # preview collection
mbx clean         # remove this workspace's managed target
```

A build outside `mise run` is only cached if you go through mise or `mbx`
directly. `mise exec -- cargo build` and the shims on `PATH` both work; a `cargo`
binary resolved from somewhere else bypasses the cache.

## Continuous integration

`.github/workflows/ci.yml` has three jobs, each of which only calls mise tasks:

| Job | Steps | Notes |
| --- | --- | --- |
| **Quality** | `mise run check` | `NEXTEST_PROFILE: ci` selects the `ci` nextest profile, which does not stop at the first failing test. |
| **Dependency audit** | `mise run deny`, then `mise run advisories` | Split into two steps so the two kinds of failure are distinguishable. No build cache: nothing is compiled. |
| **Conventional commits** | `mise run commits` | Checks out with `fetch-depth: 0`, because a shallow clone hides the history `cog check` reads. |

CI installs the toolchain with `jdx/mise-action@v5`, which resolves the committed
`mise.lock` (`mise install --locked`) and caches the installed tools; then
`jdx/mr-boxington-action@v1` restores the Cargo build cache, pinned to the same
`mbx` version as `mise.toml`. The action saves a cache entry only for a push to
the default branch, so pull requests — including forks — restore without being
able to publish.

Everything runs in CI independently of whether a contributor ran `mise run
setup` or bypassed the hooks with `HK=0`.

## Updating pinned tool versions

```sh
mise outdated                  # what has newer releases
mise up <tool>                 # upgrade an installed tool
mise use <tool>@<version>      # update the pin in mise.toml
mise lock                      # refresh mise.lock (versions, URLs, checksums)
mise install                   # install what the lockfile now says
```

`mise.lock` is committed and `locked = true` is set in `mise.toml`, so an
install that the lockfile does not cover fails instead of silently resolving a
new version. Run `mise lock` after every change to `[tools]`.

Four things move together, and none of them updates the others:

1. **Rust** — `mise.toml` (`[tools] rust`) **and** `rust-toolchain.toml`
   (`channel`, and `components` if you change them). `rust-version` in
   `Cargo.toml` is the minimum supported version and does not have to match.
   A new compiler version can also introduce new Clippy lints, so run
   `mise run ci` after bumping one.
2. **`mbx`** — `mise.toml` (`mr-boxington`) **and** the `version:` input of
   `jdx/mr-boxington-action` in `.github/workflows/ci.yml`, so one `mbx` version
   reads and writes the cache.
3. **hk** — `mise.toml` (`hk`), `min_hk_version` in `hk.pkl`, and both `amends`
   URLs in `hk.pkl`. hk does not rewrite pinned imports: a mismatch between the
   executable and the imported schema is a real failure mode.
4. **GitHub Actions** — the `uses:` versions in `.github/workflows/ci.yml` move
   independently of everything above.

## Design decisions and trade-offs

**hk 2.x, not 1.x.** `hk.pkl` targets hk 2.5.0. hk 2 removed the v1
configuration inputs (`.hkrc.pkl`, `UserConfig.pkl`, `hk generate`, non-Pkl
config files) and made a top-level `steps` map the normal way to share work
across `check`, `fix` and `pre-commit`. A v1 `hk.pkl` is not read by hk 2 — see
[the v2 migration notes](https://github.com/jdx/hk/discussions/1261). This
template defines its hooks explicitly instead of using top-level `steps`,
because the hooks need their own `fix`/`stash` settings anyway.

**Nothing here is required for `cargo build`.** Only Rust, rustfmt and Clippy
are needed to compile and test the crate. `mbx`, `hk`, `cog`, `bacon`,
`cargo-deny`, `cargo-audit`, `cargo-nextest` and `cargo-machete` are development
and CI conveniences; `mise run check` is the closest thing to a required gate,
and `cargo test` works without any of them.

**Cocogitto is scoped to commit-message validation.** It checks the message
format only: it does not write `CHANGELOG.md` or derive versions. Both are real
Cocogitto features (`cog changelog`, `cog bump`, `[bump_profiles]`), left out
because a release process is specific to a project — and because `cog bump`
would still need `cargo-edit`'s `cargo set-version` to keep `Cargo.toml`'s
version in step. Add them back in `cog.toml` plus a `mise` task when the project
has a release flow of its own.

**`bacon.toml` repeats the Clippy and test flags from `mise.toml`.** bacon runs
Cargo directly in its watch loop; it is a convenience layer and nothing in CI
depends on it, so it is not routed through `mise run`. The flags are kept
identical by hand, and commented as such.

**The first CI run and the first local `mise install` are slow.** The Cargo
subcommands are pinned and installed with `--locked`, which builds them from
source. mise caches the result locally, and `mise-action` caches it in CI, so
only the first run pays.

**Licences of unpublished crates are not checked.** `Cargo.toml` sets
`publish = false` and `deny.toml` sets `[licenses.private] ignore = true`, which
is what makes `mise run deny` pass on a fresh clone that has no licence file yet.
When you make a crate publishable, give it a `license` and set `ignore = false`.
