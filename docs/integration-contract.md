# Integration contract — native Essentia Guidance provider

**Status (30 Oct 2026): planned; no implementation or active guidance yet.**

## Reuse rather than reimplement

| Concern | Owner / source |
| --- | --- |
| Offline analysis, Essentia weights, quality and licensing provenance | [bliss-mood-poc](https://github.com/chrober/bliss-mood-poc) |
| Versioned symmetric song-pair scoring, read-only v3 SQLite feature decoding | [essentia-similarity](https://github.com/chrober/essentia-similarity), [bootstrap Draft PR #1](https://github.com/chrober/essentia-similarity/pull/1) |
| Native `bliss-guidance-jsonl-v2` process types and wire validation | [bliss-playlist-guidance-spi](https://github.com/chrober/bliss-playlist-guidance-spi) |
| Eligible candidates, bounded ranking policy, repeat, CUE/virtual constraints, route | Existing native `bliss-mixer` and `bliss-playlist-optimizer` hosts |
| Trusted provider executable, local resources, user configuration | [lms-guidance-essentia](https://github.com/chrober/lms-guidance-essentia) |

## Contract to implement

1. **`describe`**: return a truthful provider manifest, explicit version 2, channels and limits only after native binary is implemented and conforms to the SPI schema. No invented manifest or fake signals.
2. **`prepare`**: receive only host-configured trusted resource descriptors (local SQLite schema v3 with explicit user-selected model fingerprint). Open read-only and guard against schema/incompatible provenance. Resolve host identities via a *separate trusted identity artifact* with complete and verified `candidate_id -> exact feature_tracks.track_file` mapping; never infer mapping from `lms_urlmd5`, artist or title. Hash-bind and validate any frozen artifact according to SPI requirements. Preserve privacy in diagnostics. Network only when explicitly in policy, but intended provider has **zero network requests**.
3. **`score`**: receive the admitted host shortlist and actual anchor/edge context. Use only the prepared scoped identity mapping. Do not expand the candidate set, and never use single-missing-candidate partial reranking. If a candidate, anchor, model key, class ordering, source parity, or context is incompatible, return **no signals for that entire score batch** with bounded diagnostics. The host remains in sole control of final selection.
4. **`scores`**: potential future channels `essentia_genre` and `essentia_mood` are research proposals, **not registered or production-calibrated**. The SPI requires signed signals in [-1,1], confidence in [0,1], and only candidate IDs from the supplied score request; raw affine cosine [0,1] must not be treated as a perceptual confidence or automatically blended with Bliss. Active non-neutral signals are gated on independently validated musical utility and calibration. Until then, shadow-only diagnostics must not affect the native host's ranking.
5. **`close`**: free job-local memory and read-only database handles; no stored history, user audio or playlist write.

## Integration order and CI acceptance

- Exact host-owned LMS→feature key mapping, including case sensitivity, Unicode, multiple music roots, and CUE suffixes. Include wrong-key collisions and stale mappings.
- Verified feature-model provenance: `validated_against_essentia` is not an automatic musical-quality pass. Missing flag or unvalidated source remains neutral.
- SPI v2 `describe/prepare/score/close` golden transcripts compatible with actual `bliss-playlist-guidance-spi` and native host; no protocol or flag invention.
- Compare **identical frozen source/seed/candidate cohort** and all host constraints to existing Bliss order in *BlissMixerLab* shadow first; separate full-route objective for Better Call Bliss. No queue/playlist writes until explicit user action.
- Cross-platform unit/integration tests, bounded 50-candidate stress, resource use/latency on diverse devices including modest ARM hardware, failure injection, malformed JSON, duplicate candidate IDs, missing model keys, no data leaks or disk writes.
- Only after independent human-quality evidence and license/packaging review: opt-in active provider; feature-first/hybrid retrieval requires a separate host-owned API, not SPI v2.

**No active implementation is claimed by this document.**
