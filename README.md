# Bliss Guidance: Essentia (native SPI v2 provider)

## SPI-only Lyrion host boundary (31 October 2026)

The native binary remains a strictly independent `bliss-playlist-guidance-spi`
v2 provider. **The generic BlissMixerLab host does not import, understand,
prepare or invoke Essentia-specific code.** The separate
[Lyrion provider plugin](https://github.com/chrober/lms-guidance-essentia/pull/1)
owns trustworthy LMS/Bliss/Essentia key mapping, source-generation
attestation, read-only sidecar projection, provider discovery and trusted
SPI `prepare.artifacts`/`prepare.resources` configuration.

The generic host invokes the provider only by its existing native
Guidance SPI path. The earlier direct native Lab runner has been deleted,
not transferred to this repository. Native SPI v2 request and response
shapes are unchanged. There is **no completed plugin registration**,
joint-generation witness, multiple-root proof, live host scoring,
or nonempty guidance signal capability. All such requests must remain
unavailable/neutral until these gates are met.

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

### Generic Lyrion host seed alias (offline research only)

The Lab's existing Last.fm-compatible host route token, such as `seed-1`,
is distinct from the song's original `file://` URL. The provider-owned
[imported Windows SQLite-v3 pilot](https://github.com/chrober/lms-guidance-essentia/pull/1)
independently checks that the host anchor's original song URL and
`lms_urlmd5` match the trusted Lyrion track object and that its
root-relative Essentia key exactly matches the read-only Bliss catalog
and imported model snapshot. It writes the following **SHA-256-pinned**
seed row, without changing any candidate row IDs:

```json
{
  "track_id": "seed-1",
  "track_file": "Artist/Album/seed.flac",
  "source_url": "file:///host/music/Artist/Album/seed.flac",
  "lms_urlmd5": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
}
```

For `seed-N` route tokens, the Rust `prepare` handler requires these
additional provenance fields and independently matches the supplied
SPI `anchor_id`, `candidate_id`, `lms_urlmd5` and any `database_file`
to the immutable artifact. Changed or missing proofs cause fail-closed
`ANCHOR_EXACT_KEY_UNRESOLVED` / `HOST_SEED_SOURCE_PROOF_MISSING`
errors. Legacy synthetic native fixtures without the `seed-N` form
continue to use the original two-field artifact entries. No SPI-v2
wire change or new artifact version was required.

[Native CI on Linux and Windows](https://github.com/chrober/bliss-guidance-essentia/actions/runs/41217655010)
and [pinned cross-repo host/native CI](https://github.com/chrober/lms-guidance-essentia/actions/runs/41217718930)
verify good and forged aliases, including URL and hash modification
**after** the artifact is created. A SHA-256 digest provides byte
integrity, not a third-party signature or a shared Lyrion/Essentia
catalog generation witness. The score remains `signals=[]`.
## Outstanding product gates

The research-only Lyrion provider's `ImportedSidecarPilot` now produces the exact-key identity artifact for a **bounded manually imported Windows-sidecar fixture**. The normal discoverable Lyrion provider factory still returns unavailable: there is no trusted *live Lab* acquisition, installable release, authenticated shared catalog generation or approved active scoring. The installable Lyrion settings/plugin remains a separate follow-up in [`lms-guidance-essentia`](https://github.com/chrober/lms-guidance-essentia).

Actual song-level Essentia numeric source validation, human General Metric improvement, model confidence calibration, real edge/route quality, Pi runtime measurement and licensing/distribution reviews are **not complete**. Never activate score weights before these gates. Feature-first/hybrid recruitment is a separate host-owned retrieval interface outside SPI v2.

See [native integration contract](docs/integration-contract.md).

## Offline feature producer ownership

The **canonical** Rust offline scanner and original Essentia C++ worker are now maintained in [`chrober/essentia-analyser`](https://github.com/chrober/essentia-analyser) (extraction [PR #1 merged](https://github.com/chrober/essentia-analyser/pull/1)). It generates the precomputed versioned SQLite-v3 feature sidecar; no analysis binaries, ONNX models or upstream Essentia worker are launched at Guidance scoring time. [`chrober/bliss-mood-poc`](https://github.com/chrober/bliss-mood-poc) remains the **research/experimentation** repository for WP1/WP2/WP3 and historical scanner reproducibility. Actual third-party native release approval is still blocked by the producer's separate license gates.
