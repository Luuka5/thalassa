# thalassa

## Nix / flake policy (development-first)

This project is primarily used for personal development, so the Nix flake is intentionally configured for **ease of updating** over strict reproducibility.

### What this means

- The `mothership` dependency is tracked from the upstream Git repository **on the `main` branch** (i.e. “latest”).
- As a consequence, Nix builds are expected to be run in an *impure* / non-sandboxed mode so Cargo can fetch git dependencies during the build.

### How to build/run

From the repo root:

```bash
nix build --impure --option sandbox false .#
nix run   --impure --option sandbox false .#
```

### Tradeoffs

Pros:
- No manual pinning workflow during rapid iteration.
- Always uses the newest `mothership` on `main`.

Cons:
- Builds are **not reproducible** over time (the upstream git dependency can change).
- You may occasionally need to update code when upstream changes.

### If we decide to make it reproducible later

Switch to a pinned workflow:
- Keep `Cargo.lock` committed and update it intentionally.
- Add a `cargoHash` to `flake.nix` (Nix will tell you the correct hash on the first failing build).
