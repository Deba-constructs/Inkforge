# Releasing INKFORGE 1.6.0

This is the checklist for the next GitHub release. **Nothing has been pushed,
tagged or published.** Everything below was prepared locally so that publishing
is a single, reviewed command run by you (or by an assistant you explicitly tell
to push).

## What is ready in this working copy

- `Cargo.toml` and `Cargo.lock` say `1.6.0`; `build.rs` embeds that version in
  the Windows executable's version resource, and the About dialog reads it from
  `CARGO_PKG_VERSION`.
- `CHANGELOG.md` — the release notes in file form, 1.6.0 first.
- `RELEASE-NOTES-1.6.0.md` — the text to paste into the GitHub release body.
- `README.md`, `packaging/README.txt` and `DEVELOPMENT.md` describe 1.6.0.
- `dist/INKFORGE-1.6.0-windows-x86_64.zip` — built by
  `scripts/package-windows-cross.sh` (`cargo zigbuild` for
  `x86_64-pc-windows-gnu`), holding `INKFORGE.exe`, `README.txt` and
  `LICENSE-MIT.txt`. 5 800 579 bytes (5.6 MB), ZIP CRC check clean, exe
  13 282 816 bytes, PE version resource `1.6.0.0`, SHA-256
  `848f54f427ac14e7b43d3c0c942069181fdc5fc0306c82c10a198f57b907eab0`.
  A copy is in the project's `files/` folder as well.

## Before publishing

1. Re-run the suite on the commit you are about to tag:

   ```bash
   cargo check --all-targets
   cargo test  --all-targets
   ```

   Expected: no warnings, every test passing (359 at the time of writing).

2. Rebuild the package from that commit and record its size and SHA-256:

   ```bash
   bash scripts/package-windows-cross.sh
   sha256sum dist/INKFORGE-1.6.0-windows-x86_64.zip
   ```

   Put the hash into `RELEASE-NOTES-1.6.0.md` if it changed.

3. Confirm the tag does not already exist and that `1.5.0` stays untouched:

   ```bash
   git tag -l            # only backup/pre-professional-build should appear
   git log --oneline -1  # the release commit
   ```

## Publishing (needs explicit authorisation)

```bash
# 1. Commit the release files (staged by name, never `git add -A`).
git add Cargo.toml Cargo.lock CHANGELOG.md RELEASE-NOTES-1.6.0.md RELEASING.md \
        README.md DEVELOPMENT.md packaging/README.txt
git commit -m "Release 1.6.0"

# 2. Push the branch and the tag.
git push origin freebuff/21f257d93d023c7c79d308fb   # or the branch you mean
git tag -a v1.6.0 -m "INKFORGE 1.6.0"
git push origin v1.6.0

# 3. Create the release with the prebuilt ZIP and the prepared notes.
gh release create v1.6.0 dist/INKFORGE-1.6.0-windows-x86_64.zip \
   --title "INKFORGE 1.6.0" \
   --notes-file RELEASE-NOTES-1.6.0.md
```

Never force-push, never delete or modify the `v1.5.0` (Beta 3) release, and
never rewrite published history.

## What this release still cannot claim

The executable is cross-compiled on Linux. It has not been launched on Windows,
and the interface has not been looked at on a real screen or driven by a real
drawing tablet — those stay listed as verification gaps in `DEVELOPMENT.md` and
`files/REMAINING-WORK.md`. The release notes say so rather than implying the
build was run.
