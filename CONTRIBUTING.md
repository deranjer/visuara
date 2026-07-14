# Branching and releases

This repo follows GitFlow:

- `master` — always reflects the latest released code. Every commit here is tagged.
- `develop` — integration branch; everything lands here first. This is the default branch for PRs.
- `feature/*` — one branch per feature/fix, branched from `develop`, merged back into `develop` (`git merge --no-ff`) when done.
- `release/*` — cut from `develop` when preparing a release (final fixes, version bump if needed). Merged into both `master` and `develop`, then tagged on `master`.
- `hotfix/*` — urgent fixes branched from `master`, merged into both `master` and `develop`, tagged on `master`.

## Cutting a release

1. `git checkout -b release/X.Y.Z develop`, finish up any last fixes.
2. Merge into `master`: `git checkout master && git merge --no-ff release/X.Y.Z`.
3. Tag it: `git tag vX.Y.Z master && git push origin master --tags`.
4. Merge back into `develop` too: `git checkout develop && git merge --no-ff release/X.Y.Z`.
5. Delete the release branch.

Pushing a `vX.Y.Z` tag triggers `.github/workflows/release.yml`, which:
- builds the signaling server image and pushes it to `ghcr.io/deranjer/visuara:X.Y.Z` (and `:latest`)
- builds `visuara-client` natively on Windows and Linux runners (no cross-compilation — the native capture/GUI dependencies build far more reliably this way)
- attaches both binaries (`windows-x86_64.exe`, `linux-x86_64`) to a GitHub Release

To make a release's clients downloadable from your own server, download those two release assets and drop them into `docker/client-templates/` on the host running the signaling server (see `docker/client-templates/README.md`) — this is a manual step for now, there's no automated push from CI to a running server.

Every push/PR to `master` or `develop` also runs `.github/workflows/ci.yml` (build + test).
