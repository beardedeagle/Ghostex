---
name: ghostex-release
description: >-
  Release Ghostex: bump the version, write the CHANGELOG section, dispatch the
  "Release Ghostex" GitHub Actions workflow, watch it, recover failed
  platforms and verify the live release. Use when the user says "release new
  minor", "release new patch", "release new major", "cut a release", "ship
  10.x", "publish a new version", or asks to rebuild/re-publish one platform
  of an existing release.
---

# Ghostex release

Reconstructed on 2026-10-03 from releases 10.7.0 through 10.9.1 (git history,
Actions runs 36878765363, 36885477559 and 37026371059, and the tooling). Items
marked **(unverified)** were inferred and not confirmed from that evidence.

## What a release is

1. One local commit, `chore: prepare X.Y.Z`, changing exactly two files:
   `package.json` `"version"` and a new `CHANGELOG.md` section. Nothing else is
   bumped by hand: the Cargo crates stay `0.1.0`, the mobile submodule is
   untouched, and the appcast is written by CI.
2. `bun run release:actions -- X.Y.Z` dispatches `.github/workflows/release-gpui.yml`
   on `main`. Every build, signing, notarization and publish step runs on
   GitHub runners (macOS DMG + notarization + Sparkle, Linux deb/rpm/tar,
   Windows x64/arm64 Velopack + portable zip, Android APK, gxserver Linux and
   WSL runtimes).
3. Each platform has its own publish stage. The first stage to finish creates
   tag `vX.Y.Z` (at the dispatched commit) and the GitHub release. Its body
   opens with a download block of at most 3 lines (macOS + Windows, Linux,
   Android + iOS TestFlight) between `<!-- ghostex-downloads:start/end -->`
   markers, then the CHANGELOG section. Later stages amend the release and
   regenerate that block from the live assets. A failed platform does not hold
   back the others.
4. The macOS stage pushes `chore: release X.Y.Z` (author `github-actions[bot]`,
   changes `appcast.xml` only) to `main`. Releases do not touch Homebrew:
   Homebrew's autobump bot (BrewTestBot) opens the official
   Homebrew/homebrew-cask PR `ghostex X.Y.Z` itself about 30 minutes later, and
   our own PR is opt-in. Ghostex left the personal `maddada/homebrew-tap`
   (user decision 2026-10-16), so releases no longer push it.

### Who runs it, and where

Releases are started by the **Main Coordinator from Windows**
(`C:\dev\Ghostex`). Nothing needs a Mac to **ship**: signing, notarization and
publishing use repository secrets on GitHub's runners, and the dispatch only
needs `git`, `gh` (authenticated as `maddada`), `node` and `bun`. Releases up
to 10.9.1 were dispatched from the user's MacBook, so the first Windows
dispatch is new ground: if a dispatcher script breaks on Windows paths, report
the exact error rather than working around it (unverified that it runs cleanly).

The local checks that need macOS are **optional** and skipped on Windows:

- `bun run release:preflight` probes the codesigning identity and `notarytool`.
  Don't run it on Windows; the dispatcher already runs the gates that matter.
- `bun run release:verify` has DMG checks (`hdiutil`, `codesign`) and Homebrew
  checks. On Windows, pass `--skip-dmg --skip-brew --skip-repo` (step 8).
- `bun run release:test` has 14 tests that cannot pass on Windows: 10 in
  `verify-code-server-archive.test.mjs` and the 4 "rolls back a WSL Source
  install failure" tests in `release-macos-code-server-workflow.test.mjs`. Their
  fixtures need POSIX execute bits and symlinks, which Git Bash on NTFS can't
  make. The dispatcher runs the whole suite and stops on any failure, so on
  Windows run `bun run release:test` yourself first. If exactly those 14 fail
  and nothing else does, dispatch with `--skip-local-tests`. The remote `gates`
  job (macos-15) still runs the full suite before any publish stage. Any other
  failure is real: fix it before dispatching.

`release-preflight-fast.mjs` and `release-final-verify.mjs` resolve the repo
root with `new URL(...).pathname` (`/C:/...` on Windows), so even skipped they
may fail on Windows (unverified). If `release:verify` cannot start, verify with
the `gh` checks in step 8 instead.

## Procedure

### 1. Pick the version

Read `package.json` `"version"` and the latest tag
(`git tag --sort=-creatordate | head -3`; `gh release list --repo maddada/Ghostex --limit 3`).

- **minor**: `X.Y+1.0` (normal feature release; 10.7.0, 10.8.0, 10.9.0).
- **patch**: `X.Y.Z+1` (10.8.1, 10.9.1). Always +1 unless the user names a
  version.
- **major**: `X+1.0.0`.

A version whose tag already exists is refused by the dispatcher (except the
recovery flows in step 6), so a broken published release is fixed by a new
version, never by re-tagging.

### 2. Commit and push everything, and fix the release point

The dispatcher refuses unless `git status --porcelain --untracked-files=all`
is empty **and** `HEAD == origin/main`. The coordinator meets this by
committing and pushing all pending work first, not by waiting for an empty
tree:

1. Check `ghostex agents` / `ghostex sessions`. Threads in this project should
   be settled, because a thread still editing will dirty the tree again.
2. Commit and push every sub-repo with pending work (`apps/mobile/app`,
   `.dependencies/zmx`, `.dependencies/wmx`, ...), each on its own branch as
   usual. Then commit their pointer bumps in Ghostex.
3. Commit all pending Ghostex work, following AGENTS.md (formatting pass when
   quiet, no dropped hunks), then `git pull --rebase` and `git push origin main`.
   Other machines push to the same repo, so pull right before pushing.
4. Clear what remains in `git status --porcelain --untracked-files=all`:
   - ` M .dependencies/code-server` with an **unchanged** pointer (no `+` in
     `git submodule status .dependencies/code-server`) means untracked files
     inside it. On this machine that is the patch leftover
     `.dependencies/code-server/lib/vscode/build/gulpfile.reh.ts.orig`, and it
     blocks the dispatcher ("Release source is dirty"). Find what it is:
     `git -c safe.directory='*' -C .dependencies/code-server/lib/vscode status --porcelain --untracked-files=all`.
     The `-c safe.directory` is needed because the submodule is owned by
     Administrators. Delete `*.orig` / `*.rej` patch leftovers. Anything else
     inside a `.dependencies/` tree belongs to someone; stop and report it.
   - A `+` in `git submodule status` (pointer moved) is real work: commit it in
     step 2 or 3.
   - Don't hide entries with `submodule.<name>.ignore`, and don't stash or clean.
5. Write the CHANGELOG and bump the version (step 3). The pushed
   `chore: prepare X.Y.Z` commit is the **release point**: record its sha
   (`git rev-parse HEAD`) and dispatch from exactly that commit (step 4). If
   `origin/main` moves before the dispatch, the dispatcher refuses. Pull, push
   again, record the new sha, and dispatch.

Also:

- No pushes to `main` from any agent or machine until the run's `prepare` job
  passes (about 1 minute). 10.9.0's first rebuild died with
  `origin/main moved before preflight ... redispatch the release`. Later pushes
  are fine as long as they fast-forward.
- `gh auth status` is logged in as `maddada` (unverified which scopes are
  strictly needed beyond `repo` and `workflow`).
- `bun install` has been run (the dispatcher runs `bun run release:test` locally).

### 3. Write the CHANGELOG section and bump the version

Draft from the commits since the last tag:

```bash
node tooling/release-changelog-draft.mjs X.Y.Z            # draft + "omitted - confirm" list
node tooling/release-changelog-draft.mjs X.Y.Z --section-only
```

The draft is a starting point. Rewrite it by hand. Insert the section directly
under `## Unreleased`, which stays as an empty heading:

```markdown
## Unreleased

## X.Y.Z - YYYY-MM-DD

**Ghostex X.Y.Z is out.** One sentence listing the headline changes in product words.

### 📂 Files
- **Bold lead clause stating the change,** then the detail in plain words. Turn it off in Settings > Chat > <Row name>.

### 🩹 Fixes and polish
- **...**
```

The format is enforced (`validateMajorMinorReleaseNotes` in
`tooling/release-shared.mjs`, a user DECISION):

- an optional bold intro line, then `### <exactly one emoji> <theme>` headings
  that name **what changed** (`💬 Chat`, `🌐 Remote computers`, `📱 On the phone`,
  `🐧 Linux and Windows`, `🧭 Coordinators`, `🩹 Fixes and polish`). Never a
  category name (`New Features`, `Major`, `Minor`, `Stabilization`, `GPUI`), and
  never the same theme twice;
- every item is one physical `- ` line at column 0, with no nesting, no wrapping,
  and no empty group.

House style, from 10.7.0 through 10.9.1:

- Customer voice and product feature names, never file names, crates or internal
  mechanisms. Present tense ("Search highlights the words it found"). Say which
  platform when it is not all of them ("Windows: ...", "on the computer and the
  phone").
- Leave out refactors, splits, CI/release tooling, docs, tests and fixes to
  regressions that never shipped. Sign off every omission the draft lists.
- Credit outside contributors at the end of their item: `, thanks to @handle.`
- A patch that ships a previously failed platform says so in the intro ("brings
  10.9.0 to macOS"); a release that delivers a platform's missed features gets
  an item for it ("Linux gets everything from 10.7.0 in this release").
- Don't show the section to the user first: write it, commit it, and release.

Before the prepare commit, make sure generated files are current and the
typecheck passes. The remote `gates` job runs the same checks and blocks
every publish stage when they fail; builds still run and can be reused.
10.9.0 needed two extra commits regenerating
`packages/core-ui/styles/shadcn.generated.css` before it dispatched
(unverified that this is what blocked it).

```bash
cargo xtask build-sidebar-css                          # packages/core-ui/styles/shadcn.generated.css
node apps/mobile/app/scripts/generate-chat-agents.mjs  # apps/mobile/app/src/chat/session-chat-agents.generated.ts (mobile submodule)
cargo xtask typecheck                                  # also fails when skills/ghostex-help generated files are stale
```

Commit any real change the generators make, as in step 2 (the second one goes
in the mobile submodule, then its pointer bump in Ghostex). If the only diff
is line endings, it is a Windows artifact: restore that file and say so in the
report (unverified whether Windows output is byte-identical to macOS output).

Then set `"version": "X.Y.Z"` in `package.json`, and commit and push only
these two files. This commit is the release point:

```bash
git add package.json CHANGELOG.md
git commit -m "chore: prepare X.Y.Z" -m "Co-Authored-By: <the attribution line from your instructions>"
git pull --rebase && git push origin main
git rev-parse HEAD        # record: the release point
```

### 4. Dispatch

```bash
bun run release:actions -- X.Y.Z --dry-run   # validates, prints the plan and the dispatch inputs, dispatches nothing
bun run release:actions -- X.Y.Z             # real dispatch; prints the run URL
```

`release:actions` already passes `start`. Before dispatching it checks
everything in step 2, verifies the GPUI reference contract and the Ghostty Zig
pin, runs `bun run release:test` (about 11 seconds), checks the repository
secrets, and previews the change-aware plan (build, reuse or skip per product).
A normal release (10.9.1) uses **no flags**: every platform is in scope,
Sparkle is on, and Windows signing is `auto`: on when the
`SIGNPATH_API_TOKEN` secret exists (SignPath, `docs/code-signing.md`), otherwise
Windows ships unsigned.

Scope flags, only on the user's instruction: `--skip-macos --skip-sparkle`,
`--skip-linux`, `--skip-windows`, `--skip-android`, `--only-macos`,
`--prerelease` (needs `--skip-sparkle`), `--force-all` or `--force a,b`
(rebuild even when the inputs are unchanged).

The tag is created at the dispatched commit, which is HEAD at dispatch and not
necessarily the prepare commit (v10.9.0 is at 9f8d3bb6e, two commits after
`chore: prepare 10.9.0`).

### 5. Watch

A full run takes about 50 minutes (10.9.1: dispatched 15:21, Linux live 15:46,
Android 15:57, macOS and Windows about 16:10). Watch without flooding the
context:

```bash
bun run release:watch -- --run <run-id> --exit-on-change --interval 120   # run in the background; re-launch after each exit
```

Job shape worth knowing while watching (since 2026-10-07, first shipped with
the release after 10.14.0; watch that one closely):

- Each Windows architecture is two jobs. `windows_<arch> / native runtime (<arch>)`
  compiles gxserver, ghostex, wmx and the prompt editor (~12 min) and uploads
  them as `release-windows-native-runtime-<arch>`; `windows_<arch> / build`
  compiles the desktop app at the same time, then waits for that artifact
  ("Await the native runtime binaries of this run"), downloads it and packages.
  If the runtime job fails, the build job fails at that wait with the artifact
  list; read the runtime job's log, not the build job's.
- macOS compiles gxserver before it waits for the Linux runtime artifacts, so
  "Await the runtime artifacts of this run" now comes after
  "Cargo build gxserver" and is usually short.

Exit codes: `0` finished, `1` a job failed, `2` `--max-minutes` elapsed, `3` `gh` failed five polls in a row. With
`--exit-on-change`, a stdout line `run <id> completed:` means the run is over,
while any other exit `0` means only that a job changed state.

### 6. When a job fails

1. Read only the failed job's log: `gh run view <run-id> --repo maddada/Ghostex --json jobs`
   for the job id, then `gh api repos/maddada/Ghostex/actions/jobs/<job-id>/logs > job.log`.
2. Classify it: `bun run release:classify -- job.log` prints `TRANSIENT` (exit
   0, a network/runner hiccup), `FATAL` (exit 1, compiler, signature, integrity
   or test failure) or `CANCELLED` (exit 2, check whether a sibling failed first).
3. **Transient, or fatal and fixed on `main`** while the tag does not exist yet,
   or the tag exists from a partial staged publish: redispatch the same version
   reusing everything that already built:
   `bun run release:actions -- X.Y.Z --reuse-from-run <failed-run-id>` (add
   `--skip-*` flags to limit the scope to the failed platforms). Its stages
   amend the live release.
   A Windows platform is reused or rebuilt as a whole: reuse only ever reads
   the `windows_<arch> / build` job's provenance, so a failed
   `native runtime (<arch>)` job means its `build` job failed too and both
   rerun on the redispatch. Re-running only the failed `build` job in the same
   run also works when its runtime job succeeded: the runtime artifact stays
   on the run (unverified: no Windows job has been re-run this way yet).
4. **`gates` failed only** (typecheck/test): fix it, push, and redispatch with
   `--reuse-from-run`. Builds are reused, and only the publish stages waited on gates.
5. **Every product built but publishing failed**: re-run the publish only,
   from the recorded plan: `bun run release:actions:publish -- X.Y.Z --source-run-id <run-id> [--stage macos|android|linux|windows-x64|windows-arm64]`.
6. **A platform failed and the release is already public** (another stage
   created the tag): rebuild **that platform from the release's own sources**.
   Don't ask; this is the user's standing choice. Builds always come from
   current `main`, so main's tree is set to the release's sources for one dispatch:
   1. Fix the failure on `main` first (normal commit, pushed), so the fix is
      known to work.
   2. Make a commit whose tree is the tag's tree plus only that fix. With a
      clean tree (step 2; this rewrites every tracked file, so never run it over
      someone's uncommitted work):
      ```bash
      git restore --source=vX.Y.Z --staged --worktree -- .   # also removes files added after the tag
      git cherry-pick --no-commit <fix-sha>                  # resolve by hand if it conflicts
      git diff --cached --stat vX.Y.Z                         # only the fix's files may differ
      git commit -m "chore(release): restore the X.Y.Z sources on main for the <Platform> rebuild"
      git push origin main
      ```
      Submodule pointers (`apps/mobile/app`, `.dependencies/*`) go back with the
      tree, as they did in 10.9.0.
   3. Dispatch only the failed platform, reusing everything else from the
      failed run. The 10.9.0 Windows scope (run 36885477559) was
      `bun run release:actions -- X.Y.Z --skip-macos --skip-sparkle --skip-linux --skip-android --reuse-from-run <failed-run-id>`.
      Swap the `--skip-*` flags for the platform you need. A macOS rebuild keeps
      Sparkle on, since its appcast entry was never written (unverified: no
      macOS rebuild has been done this way).
   4. Wait until that run's `prepare` job has **succeeded**, then
      `git revert --no-edit HEAD` (the restore commit) and push. That gives
      `Revert "chore(release): restore ..."`. Reverting before `prepare` passed
      killed the first 10.9.0 attempt (run 36884637494). Nobody may push in that window.
   5. Watch the rebuild to the end, then check the release's assets (step 8).

   Earlier releases sometimes shipped the next patch instead (10.9.0 macOS →
   10.9.1). Do that only if the restore-and-rebuild fails twice, and say so in
   the report. The same-version amend workflow (`release-amend-existing.yml`)
   builds current `main` under the old version number; don't use it.
7. Never move or delete a public tag, and never re-run a whole release for a
   version that is already live.

### 7. Publish side effects (automatic; check they happened)

- **Sparkle (macOS auto-update)**: the macOS stage commits `appcast.xml` as
  `chore: release X.Y.Z` to `main`. Run `git pull --ff-only` afterwards.
- **Windows auto-update**: Velopack `releases.win-*-stable.json`, `RELEASES-*`
  and `*.nupkg` assets on the GitHub release (`windows-update-feed.mjs`).
- **Android**: `ghostex-android.apk` on the GitHub release, built from the
  `apps/mobile/app` submodule at the dispatched commit.
- **Homebrew**: the official cask is updated by Homebrew's autobump bot, which
  opens the `ghostex X.Y.Z` PR on Homebrew/homebrew-cask (e.g. #291473 for 10.9.1)
  within about half an hour of the release; our workflow no longer opens one (user
  decision 2026-10-04; its token had no write access to the fork and every run since
  10.0.1 failed with HTTP 404). There is no Homebrew job and no Homebrew token
  any more, and the personal `maddada/homebrew-tap` is not touched (its Ghostex
  cask is deprecated; user decision 2026-10-16). Check the official cask with
  `gh pr list --repo Homebrew/homebrew-cask --search ghostex --state all --limit 3`.
  If no PR appears within a few hours, open it ourselves: `HOMEBREW_GITHUB_API_TOKEN=... node tooling/release-gpui/publish-homebrew-cask.mjs --version X.Y.Z --publish --official`
  (`tooling/release-gpui/homebrew-cask-setup.md`).
- **AUR (`ghostex-bin`)**: never touch it. Another person updates it
  automatically, and the Linux stage's AUR step skips itself because
  `AUR_SSH_PRIVATE_KEY` is not set. Don't run `bun run release:aur`, don't add
  the secret, and don't report on it.
- **Component tags** (code-server, CEF) go to the components repo with
  `GHOSTEX_COMPONENTS_GITHUB_TOKEN`. They are automatic.

### 8. Verify

```bash
gh release view vX.Y.Z --repo maddada/Ghostex --json isDraft,isPrerelease,assets --jq '.isDraft,.isPrerelease,(.assets[].name)'
```

Expect about 24 assets: `ghostex-X.Y.Z-arm64.dmg`, `ghostex_X.Y.Z_amd64.deb`,
`ghostex-X.Y.Z-1.x86_64.rpm`, `ghostex-X.Y.Z-linux-x64.tar.zst`,
`ghostex-X.Y.Z-windows-{x64,arm64}.exe` and `-portable.zip`, the Velopack
files, `ghostex-android.apk`, `gxserver-linux-{x64,arm64}.tar.gz`,
`gxserver-wsl-windows-{x64,arm64}.zip` and `release-provenance-X.Y.Z.json`.

The body must start with the download block, and every customer asset
(DMG, both Windows installers and portable zips, deb, rpm, tarball, APK) must
be linked there; `release:verify` checks both. Checksums, Velopack files,
gxserver runtimes and provenance stay as unlisted assets.

From Windows (the normal case), skip the macOS-only DMG and Homebrew checks.
Also skip the repo check, because `main` has moved past the tag by now:
`bun run release:verify -- X.Y.Z --run-id <run-id> --skip-dmg --skip-brew --skip-repo`.
It prints a PASS/FAIL table covering provenance, Sparkle and Android. Pull
`main` first: `changelog-section` reads the local `CHANGELOG.md`. On Windows
`live-appcast` is a WARN, not a FAIL, because xmllint is missing there (the
appcast fields are still checked; only well-formedness is not). A
`provenance` or `planned-products-live` FAIL naming products whose assets
are live means their records are missing from `release-provenance-X.Y.Z.json`;
report it, since the next release's planner cannot reuse those products.
Optional, on a Mac after `brew update`, the full check with no skip flags:
`bun run release:verify -- X.Y.Z --run-id <run-id>`.

### 9. Report to the user

- the version, the run URL, the release URL, and the wall time;
- each platform: live, reused or failed, plus what you did about each failure;
- the release-point sha, the Sparkle appcast commit, and the Homebrew PR (number and state);
- anything left for them: a missing Homebrew PR, or a platform whose
  rebuild failed twice;
- a reminder that `main` received the bot's `chore: release X.Y.Z` commit, so
  every machine should pull.

## Don'ts

- Don't build, sign or notarize locally, and don't install into `/Applications`.
- Don't hand-edit `appcast.xml` or the casks.
- Don't make the tree clean by stashing, cleaning, resetting or ignoring
  other people's work: commit and push it (step 2). Only `*.orig`/`*.rej`
  patch leftovers inside `.dependencies/` get deleted.
- Don't push to `main` during the first minutes of a run, before `prepare` passes.
