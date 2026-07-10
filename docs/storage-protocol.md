# sb-netplay-serv Storage Protocol

## Repository identity

This service is an independent Git repository whose physical root is:

```text
/Volumes/code-bank/code/sb-desktop/sb-netplay-serv
```

The Shadowboy Desktop V2 `services/sb-netplay-serv` entry is only a navigation
symlink. V2 storage initialization, cleanup, builds, and tests must not recurse
into this repository. Work here deliberately and inspect this repository's own
Git state first.

The machine-wide v0.2 artifact-only contract is authoritative:

```text
/Volumes/code-bank/policies/project-storage-protocol.md
/Volumes/code-bank/policies/build-cache-artifact-layout.md
/Volumes/code-bank/policies/existing-project-migration-brief.md
```

## Canonical paths

| Storage class | Path |
| --- | --- |
| Source | `/Volumes/code-bank/code/sb-desktop/sb-netplay-serv` |
| Project cache | `/Volumes/code-bank/caches/sb-netplay-serv` |
| Build root | `/Volumes/code-bank/build/sb-netplay-serv` |
| Cargo target | `/Volumes/code-bank/build/sb-netplay-serv/rust-target` |
| Node output | `/Volumes/code-bank/build/sb-netplay-serv/node` |
| Gradle output | `/Volumes/code-bank/build/sb-netplay-serv/gradle` |
| Package staging | `/Volumes/code-bank/build/sb-netplay-serv/package-staging` |
| Dev artifacts | `/Volumes/code-bank/artifacts/sb-netplay-serv/dev` |
| Nightly artifacts | `/Volumes/code-bank/artifacts/sb-netplay-serv/nightly` |
| Releases | `/Volumes/code-bank/artifacts/sb-netplay-serv/release/<version>` |
| Diagnostics | `/Volumes/code-bank/artifacts/sb-netplay-serv/diagnostics` |
| Logs | `/Volumes/code-bank/logs/sb-netplay-serv` |
| Scratch | `/Volumes/code-bank/tmp/scratch/sb-netplay-serv` |

There is no top-level `/Volumes/code-bank/releases` namespace. Do not recreate
the legacy `dev/nightly/stable/diagnostics` release tree.

## Parent-owned components

The following buildable directories belong to this repository's storage owner:

```text
sdk/typescript
sdk/kotlin
```

They are not independent storage projects. They must not carry nested
`.project-storage.toml`, `.envrc.example`, storage docs, or cleanup scripts.
Their dependencies remain rebuildable, and their build/cache output uses the
parent paths above. The Kotlin build directory is configured under
`$PROJECT_BUILD_DIR/gradle/sdk-kotlin`; TypeScript checks use `--noEmit`.

## Environment contract

`scripts/project-storage-env.sh` is the single path owner. It validates the
physical checkout and external filesystem, rejects conflicting path overrides,
and exports the project, Cargo, Bun/npm, Gradle, and compiler-cache variables.
It performs no directory creation.

For one command:

```sh
scripts/with-project-env.sh cargo test
scripts/with-project-env.sh cargo run
scripts/with-project-env.sh bun test sdk/typescript/tests/**/*.test.ts
```

For an interactive shell, review `.envrc.example`, copy it to the ignored
`.envrc`, and approve it with direnv. Restore dependencies only through their
manifests and package managers; do not recover old generated trees.

## Docker ownership

Docker runs through the external-backed OrbStack data root. `/app/target` in the
Dockerfile is an ephemeral path inside the image build stage, not host source
output. The build script validates this repository's external storage contract
before invoking Docker. Named volumes and container runtime data are never
generic cleanup targets. Pushed GHCR images are deployment deliverables; any
exported image archives or diagnostics belong under the artifact tree.

## Existing local output

The repository had a legacy source-local `target/` when this contract was
migrated. It is rebuildable output, not source, but migration did not delete or
move it. Future Cargo commands must use the canonical external target. Review any
later cleanup separately; do not bypass the v0.2 cleanup safety guard.

## Safe maintenance

```sh
scripts/ensure-project-storage.sh
project-storage-check /Volumes/code-bank/code/sb-desktop/sb-netplay-serv
scripts/space-report
scripts/clean-soft
scripts/clean-deep
scripts/rust-target-report
scripts/rust-target-prune
```

`project-diet prune` is read-only in v0.2. `clean-deep` deliberately performs
only a code-only scan. Never delete source, tracked files, secrets, databases,
analytics data, named Docker volumes, or other runtime data through these
helpers.
