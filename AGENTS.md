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

`main` is protected: direct pushes are refused, for admins too. Every change
lands through a pull request whose CI checks pass. The repository allows merge
commits only — squash and rebase merging are disabled — so the explicit merge
commit above survives a merge made through the GitHub UI.

`.github/workflows/ci.yml` runs on every pull request and on `main`: tests,
doctests, `fmt --check`, `clippy -D warnings`, `cargo doc` with warnings denied,
a build against the declared MSRV, and a packaging dry run that also asserts
AGENTS.md stays out of the tarball.

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

4. Open a pull request and let CI pass.

### Merging the bump is the release

There is no manual publish step. `.github/workflows/release.yml` watches `main`,
finds a version whose tag does not exist yet, and then, in this order:

1. Re-runs `fmt`, `clippy` and the tests. Publishing cannot be undone, so the
   gate runs again rather than trusting that CI on the same commit has finished.
2. `cargo publish`.
3. Tags the published commit `v<version>` and pushes the tag.
4. Creates the GitHub release from that tag.

The irreversible step is deliberately first: a failure at publish then leaves no
tag or release claiming a version that was never published.

Every other merge to `main` is a no-op — the workflow stops the moment it finds
the tag already present. So a release is exactly "merge a version bump", and
nothing else can trigger one by accident.

Release runs are serialised by a `concurrency` group. Two merges landing
together would otherwise both check out before either pushed a tag, see the same
version untagged, and race to publish it. A run in progress is never cancelled,
because it may be mid-publish.

This needs a `CARGO_REGISTRY_TOKEN` repository secret. Use a crates.io token
created for CI and scoped to publish-update for this crate, not a copy of a
local credential.

docs.rs builds asynchronously afterwards. A 404 immediately after publishing is
normal; if it persists, read `https://docs.rs/crate/twenty48/<version>/builds`.

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
  it is the only cheap way to verify an MSRV claim locally, and it has already
  caught a real incompatibility. CI additionally builds against that exact
  toolchain, so the claim is checked rather than inferred.
- **Publishing the same version twice fails.** If a release run has to be redone,
  bump the version rather than deleting the tag — the crates.io version is gone
  either way, since a version can be yanked but never replaced.
