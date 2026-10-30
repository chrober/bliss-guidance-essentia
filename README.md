# Bliss Guidance: Essentia (native SPI v2 provider)

**Status:** working native **research-only, shadow-neutral SPI v2 implementation** on the experimental branch `feat/essentia-guidance-integration-design` / [Draft PR #1](https://github.com/chrober/bliss-guidance-essentia/pull/1). **Not yet an active guidance model or shipped Lyrion integration.**

This repository follows the [`bliss-guidance-lastfm`](https://github.com/chrober/bliss-guidance-lastfm) and [`bliss-guidance-library-signals`](https://github.com/chrober/bliss-guidance-library-signals) naming convention. The `bliss-` prefix identifies the host Guidance SPI, **not a dependency in the mathematical similarity engine**. The reused engine lives in [`essentia-similarity`](https://github.com/chrober/essentia-similarity), already merged independently.

## What has been implemented

- Native Rust executable `bliss-guidance-essentia` speaking the **real**, version-pinned [`bliss-playlist-guidance-spi`](https://github.com/chrober/bliss-playlist-guidance-spi) JSONL protocol v2: `describe` / `prepare` / `score` / `close`. It supports `version` and `version --json` metadata.
- The provider declares one **experimental shadow channel** with **no supported active host policy** and always returns an **empty `signals` list**. This deliberately prevents Bliss or the route optimizer from changing song selection in the absence of a scientifically validated General Metric.
- `prepare` accepts an explicitly pinned feature model, versioned cosine metric and optional experimental shadow-only use of unvalidated model sources. It requires **one trusted read-only** SQLite-v3 resource and **one SHA-256-verified**, bounded (`<=256 KiB`) exact-key identity artifact tied to the job ID.
- The identity artifact is versioned `essentia-exact-track-keys-v1`, a host-produced mapping of `track_id` to **exact** `feature_tracks.track_file` keys for anchored tracks and already-admitted candidates. No `urlmd5`→filename, artist/title, Unicode folding, CUE or root guessing takes place here.
- `score` verifies a complete bounded batch (2–50 candidates), one known global context anchor, no ID duplicates, exact candidate-key consistency, source/model fingerprints and class label ordering. It calls the **actual reusable Rust geometry** on genuinely decoded source-compatible SQLite vectors, then returns **only privacy-safe coverage diagnostics**, still with zero active signals.
- Missing/unvalidated rows, wrong identity, fingerprint, invalid geometry, unsupported directed edge context or incompatible snapshot causes a **neutral whole-query fallback**. Input and error diagnostics never include filenames, song IDs, user metadata or raw scores. No ONNX/audio analysis and no playlist/sidecar writes.
- Tests use synthetic tracks in actual temporary SQLite-v3 databases. Additional tests exercise the native JSONL executable via stdin/stdout, including malformed and unprepared input, not merely internal handler methods. CI runs on Linux and Windows.

## Build / inspect

```shell
cargo test --all-targets
cargo fmt --all -- --check
cargo run -- version --json
```

Normal process mode reads one JSONL request per line from stdin and writes one bounded SPI-v2 response per line to stdout. The **native host**, not a Lyrion web browser request, must supply trusted configured resources/artifacts. Do not treat manually constructed local request JSON as a production provider configuration.

### Exact-key artifact (trusted host-produced, fictional example)

```json
{
  "schema": "essentia-exact-track-keys-v1",
  "schema_version": 1,
  "job_id": "host-job-123",
  "tracks": [
    {"track_id": "source-1", "track_file": "Artist/Album/seed.flac"},
    {"track_id": "candidate-1", "track_file": "Artist/Album/example-one.flac"},
    {"track_id": "candidate-2", "track_file": "Artist/Album/example-two.flac"}
  ]
}
```

The host must send an SPI `prepare.artifacts` descriptor with `kind=essentia-exact-track-keys-v1`, the artifact's trusted local path and **its actual SHA-256**, and `prepare.resources` with `kind=essentia-feature-sqlite-v3`, trusted local absolute SQLite-v3 path and `access=read_only`. `prepare.anchors` must contain source anchor IDs present in that mapping. `prepare.options` supplies `model_id` (`genre` or `moodtheme`), `metric` (`affine_cosine_v1` or `affine_centered_cosine_v1`), `expected_model_key`, and optional `experimental_preview_opt_in` (default false). Only global score contexts with one matching `context_track_ids` entry are evaluated. The result is **always shadow-neutral**; no active scores or alternative order are published.

## Outstanding product gates

The existing [BlissMixerLab](https://github.com/chrober/lms-blissmixer-lab) / [Better Call Bliss](https://github.com/chrober/lms-better-call-bliss) hosts **do not currently produce this exact-key identity artifact**. The verified host-owned mapping is therefore the next practical integration dependency. The Lyrion settings/installable plugin is separately developed in [`lms-guidance-essentia`](https://github.com/chrober/lms-guidance-essentia).

Actual song-level Essentia numeric source validation, human General Metric improvement, model confidence calibration, real edge/route quality, Pi runtime measurement and licensing/distribution reviews are **not complete**. Never activate score weights before these gates. Feature-first/hybrid recruitment is a separate host-owned retrieval interface outside SPI v2.

See [native integration contract](docs/integration-contract.md).
