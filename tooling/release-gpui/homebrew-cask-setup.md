# Homebrew cask: the manual `--official` fallback

**Releases do not touch Homebrew.** The official `ghostex` cask, `Casks/g/ghostex.rb`
in [Homebrew/homebrew-cask](https://github.com/Homebrew/homebrew-cask), is the only
advertised macOS install (`brew install ghostex`), and Homebrew's autobump bot
(BrewTestBot) opens its `ghostex <version>` bump pull request about 30 minutes
after each release (10.24.0: #294691). A normal release needs no Homebrew token.

- **Since 2026-10-04 our own official-cask pull request is opt-in** (`--official`;
  user decision): this token's write access to the fork failed with HTTP 404 on
  every release from 10.0.1 on while BrewTestBot opened every bump itself.
- **Since 2026-10-16 releases no longer push the personal tap** (user decision):
  Ghostex left [maddada/homebrew-tap](https://github.com/maddada/homebrew-tap). Its
  `Casks/ghostex.rb` was bumped one last time to 10.24.0 and deprecated in favour
  of `brew install --cask ghostex`; existing tap installs keep updating through
  Sparkle. The tap still serves `codex-swap`, which the codex-swap release
  workflow publishes.

The release workflow has no Homebrew job any more. Everything below describes
`tooling/release-gpui/publish-homebrew-cask.mjs --official`, kept for the rare
case where the bot's pull request does not appear: it opens a version-bump pull
request from the `maddada/homebrew-cask` fork on a `ghostex-<version>` branch,
titled `ghostex <version>`, changing `version` and `sha256` only. The sha256 is
the digest GitHub records for `ghostex-<version>-arm64.dmg`, cross-checked
against `release-provenance-<version>.json` when the release carries one.
Homebrew's pull request template asks for `brew style` and `brew audit --cask
--online`; on a Mac, render the bumped cask with `--render <path>`, run both on
it in a throwaway local tap, and pass `--audited --styled` only after they pass
(the retired workflow audit step that did this is in git history, in
`.github/workflows/release-gpui-publish.yml` before 2026-10-16).

## Token: `HOMEBREW_GITHUB_API_TOKEN`

One GitHub token for the `maddada` account, needed only for `--official
--publish`. Pass it in the environment when running the script by hand; no
workflow reads it any more, so it does not need to be a repository secret.

Fine-grained personal access token (preferred):

- Resource owner: `maddada`.
- Repository access: **Only select repositories**, pick `maddada/homebrew-cask`.
- Repository permissions: **Contents: Read and write**, **Pull requests: Read and
  write**, **Metadata: Read** (added automatically).
- No account permissions, and no **Workflows** permission (the `workflow` scope
  of a classic token). The script never copies upstream commits into the fork,
  so it never has to write under `.github/workflows`; see the fork section.

A fine-grained token opens the pull request against `Homebrew/homebrew-cask`
even though that repository is not in its list: creating a PR only needs write
access to the fork branch and read access to the public upstream.

Classic token alternative: scope `public_repo` only. It grants push and PR rights
on every public repository the account can write to, so prefer the fine-grained
token.

The token must belong to the account that owns the fork: the script derives the
fork owner from `GET /user` with this token and pushes the bump branch there.

## Fork of Homebrew/homebrew-cask

`maddada/homebrew-cask` already exists (it carried the `add-ghostex` submission).
The script forks the repository itself if it is missing. It never syncs the
fork: the `ghostex-<version>` branch is created (or force-reset, on a retry) at
the head commit of upstream's default branch, which the fork can reference
because a fork shares upstream's object network, and the bump is committed on
top of that commit. The fork's own `main` may fall arbitrarily far behind and
nothing reads it. Feature branches such as `add-ghostex` do not interfere.

The fork used to be fast-forwarded with the merge-upstream API before
branching. Release 9.7.0 failed there with HTTP 422, "refusing to allow a
Personal Access Token to create or update workflow
`.github/workflows/check-issues.yml` without `workflow` scope": that sync
copies every upstream commit into the fork, including Homebrew's frequent
workflow edits, which a token limited to contents and pull requests may not
write. Do not bring the sync back or widen the token for it; the pull request
never depends on the fork's `main` being current.

## Autobump

BrewTestBot checks the cask's `livecheck` source (`appcast.xml` on `main`) and
opens `ghostex <version>` itself when it sees a newer version. Check it with
`gh pr list --repo Homebrew/homebrew-cask --search ghostex --state all --limit 3`.
The script never collides with it: it exits 0 with the existing pull request's
URL when one is open for the version, whoever opened it, and also exits 0 when
the live cask already carries the version.

## Running it by hand

```sh
node tooling/release-gpui/publish-homebrew-cask.mjs --version 9.5.1 --official --dry-run
HOMEBREW_GITHUB_API_TOKEN=... node tooling/release-gpui/publish-homebrew-cask.mjs --version 9.5.1 --publish --official
```

`--dry-run` prints the exact diff and performs no writes. `--sha256 <hex>`
overrides the digest lookup. Without `--official` or `--render` the script
refuses to run, because there is nothing else left for it to update.
