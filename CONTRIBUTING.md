# Contributing

Waldo is a public library. Everything that lands here is public, so keep personal data out of code, tests, fixtures, commit messages, and pull request text: no home paths, email addresses, names, customer data, or machine identifiers.

Everyone who takes part follows the [code of conduct](CODE_OF_CONDUCT.md). A way past a gate, the stop switch, or the audit log is a security bug. Report it privately, as [SECURITY.md](SECURITY.md) says, and not in a public issue.

## Branches

`main` is what people install. It changes only through pull requests, and CI must pass.

| Branch | For |
|---|---|
| `ui/<name>` | Windows people see: the bar, onboarding, the gate panel |
| `core/<name>` | The tools, the gate rules, the MCP server |
| `docs/<name>` | README and docs only |
| `ci/<name>` | Workflows and build scripts |

## The pipeline

1. Branch from `main`: `git switch -c ui/<name> origin/main`.
2. Push early and open a **draft** pull request. CI runs on every push.
3. CI builds every crate, test target, and example. It runs the unit tests, the lint that stops OS state being read by parsing program output, and a build of the gate panel. It also checks added lines for home paths and personal email addresses.
4. CI cannot run the live checks, because they need Accessibility and Screen Recording grants. Run `make live` on your Mac and say so in the pull request.
5. A UI change shows a screenshot or a short GIF in the pull request.
6. When the work is ready, mark the pull request ready for review. Merge with **rebase**, so `main` keeps a straight history. GitHub's merge buttons record the merger's account email as the committer. Before you merge on the web, turn on "Keep my email addresses private" in GitHub's email settings. Or fast-forward from your machine: rebase the branch on `origin/main`, then `git push origin <branch>:main`. GitHub marks the pull request merged.
7. A change people will notice adds a line under `Unreleased` in `CHANGELOG.md`.

## UI branches

A UI branch can carry a runnable example under `crates/waldo-bar/examples/`. Run it with `cargo run -p waldo-bar --example <name>`. CI builds every example, so a broken one fails the pull request.

## Releases

Versions follow [SemVer](https://semver.org/). The MCP server reports the workspace version to clients, so the version in `Cargo.toml` is the one people see.

1. On a `docs/release-<version>` branch, set `version` in the root `Cargo.toml`, then run `cargo build` to update `Cargo.lock`.
2. In `CHANGELOG.md`, move the `Unreleased` lines under a new version heading with the date, and update the links at the bottom.
3. Open a pull request, wait for CI, and merge with rebase.
4. Tag the merge on `main` and push the tag: `git tag -a v<version> -m "Waldo <version>"`, then `git push origin v<version>`.
5. Publish a GitHub release from the tag, with that version's changelog entry as the notes.

## Commits

- One commit per idea. The message says what changed and why.
- Use your GitHub `noreply` address as the commit email (GitHub, Settings, Emails), so your personal address stays private.
- A change to the gates needs a unit test in `gate.rs`. A change to a tool needs a live check.
