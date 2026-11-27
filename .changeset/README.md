# Changesets

Every pull request, and every commit pushed straight to `main`, adds one file here describing what it changes for the people who use the software in this repository. [Changesets](https://changesets.dev) turns those files into version bumps and `CHANGELOG.md` entries when the "Version Packages" pull request is merged.

```sh
mise run changeset              # pick the bump (patch / minor / major) and write the summary
mise run changeset -- --empty   # nothing releasable changed; say so explicitly
mise run version                # apply pending changesets locally (CI normally does this)
```

Write the summary for a reader of the changelog, not for the reviewer of the diff: what they can now do, what no longer works, what they must change. Workbench's users adopt its process, so the audience is the person who copied this kit into their own repository and now has to update their copy.

This repository has one version, carried by the private root `package.json`, even though nothing here is published to npm. There is no second manifest to sync it into: `mise run version` bumps it and writes `CHANGELOG.md`, and the release workflow tags the commit that lands the bump.
