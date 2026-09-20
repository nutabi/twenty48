# AGENTS.md

## Git

### Commit

Title is limited to 72 characters and has the format `<type>: <summary>`, where is one of:

- `meta`: Changes to project meta-source code (build scripts, CI/CD, version control, etc.)
- `feat`: New or changed functionality in the engine or its interface
- `fix`: Corrections to existing behaviour
- `docs`: Documentation only

Commit body can have more details. Never add `Co-Authored-By` or other irrelevant information.

More can be added as needed.

### Branch

Never work on `main`. All branch will be feature-complete (passed test cases, etc.) before being merged to `main`.

`main` is never fast-forwarded. Merge commits are explicit.

## Release

A release is cut from `main` but never *committed* on it: the version bump is an
ordinary branch that gets merged. Publishing and tagging only read `main`, so
those are done from it directly.

### Three version numbers, independently owned

| Number | Where | Bumped when |
| --- | --- | --- |
| Crate version | `version` in `Cargo.toml` | Every release. Semver — while on `0.x`, a breaking change bumps the minor. |
| Protocol revision | `PROTOCOL` in `src/protocol.rs` | Only a change that breaks existing clients. Ideally never. |
| Notation version | `VERSION` in `src/notation.rs` (`g1`) | Only a change to the game-string format, and always as a new tag rather than a redefinition of the old one. |

Releasing the crate does not imply bumping the other two, and usually must not.

### On a branch

1. Bump `version` in `Cargo.toml`.
2. Update the `id` example in `README.md` and `PROTOCOL.md`. It embeds the crate
   version and goes stale on every bump. The code is safe — the test asserts
   against `CARGO_PKG_VERSION` — so only the prose drifts.
3. Run the whole gate, and require all of it to be clean:

   ```sh
   cargo test
   cargo clippy --all-targets -- -D warnings
   cargo fmt --check
   cargo doc --no-deps      # what docs.rs will build
   cargo package --list     # what actually ships
   cargo publish --dry-run
   ```

4. Merge into `main` with `--no-ff`, then push.

### From `main`

5. Confirm the tree is clean and in sync with `origin/main`. Cargo refuses to
   package uncommitted changes, so a clean tree is what makes the published
   source equal to the commit.
6. `cargo publish`.
7. Confirm the published commit is the one intended, before tagging it:

   ```sh
   cat target/package/twenty48-<version>/.cargo_vcs_info.json   # sha1 == HEAD
   ```

8. Tag that commit, annotated, and push the tag:

   ```sh
   git tag -a v<version> -m "twenty48 <version>"
   git push origin v<version>
   ```

9. Create the GitHub release from the tag:

   ```sh
   gh release create v<version> --title "twenty48 <version>" --notes "..."
   ```

10. docs.rs builds asynchronously. A 404 immediately after publishing is normal;
    if it persists, read `https://docs.rs/crate/twenty48/<version>/builds`.

### Things that have bitten before

- **A publish is permanent.** A version can be yanked but never deleted, and the
  name is claimed for good. Get it right before uploading, not after.
- **`cargo publish --dry-run` stops before upload**, so it never checks whether
  the name is free or owned by you. Before a first publish under any new name,
  query crates.io directly — a green dry run proves nothing about the name.
- **`cargo package --list` fails on a dirty tree** and prints nothing to stdout.
  Grepping that empty output passes for the wrong reason; confirm the listing is
  non-empty before trusting what it says about excluded files.
- **`rust-version` is what enables clippy's `incompatible_msrv` lint.** Declaring
  it is the only cheap way to verify an MSRV claim without installing that
  toolchain, and it has already caught a real incompatibility.
