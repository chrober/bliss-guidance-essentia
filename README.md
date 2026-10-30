# Bliss Guidance: Essentia (native provider)

**Status: integration-design scaffold only. No executable SPI v2 provider is implemented or shipped.**

This repository will implement a native Rust **Bliss Guidance SPI v2 provider** using the host-independent [`essentia-similarity`](https://github.com/chrober/essentia-similarity) crates. It will provide optional Essentia-derived Genre/Mood similarity signals for *already host-qualified* candidates. Host candidate admission, repeat windows, virtual tracks and route feasibility remain outside this provider.

The counterpart installable Lyrion package is [`lms-guidance-essentia`](https://github.com/chrober/lms-guidance-essentia). The offline scanner and independent general/personal metric research remain [`bliss-mood-poc`](https://github.com/chrober/bliss-mood-poc). The normative process protocol is maintained by [`bliss-playlist-guidance-spi`](https://github.com/chrober/bliss-playlist-guidance-spi), not duplicated here.

## Planned provider

- Receive native host's `describe -> prepare -> score -> close` JSONL lifecycle.
- `prepare`: verify trusted local, read-only Essentia SQLite v3 resource; verify pinned model IDs, exact `feature_tracks.track_file` identities and independent quality/reference validation, freeze a bounded session. **No browser-supplied path or arbitrary file access.**
- `score`: only inspect the *same bounded, previously qualified candidate pool* supplied by the host, using exact host-resolved opaque feature keys. Network-free and inference-free; no full-library scanning.
- Unavailable/mismatched validation, identity, labels, vectors or score coverage => **neutral whole-batch fallback**. No candidate ID or feature payload leaks to logs. No route, playlist or queue writes.
- A provisional normalized cosine/centered-cosine geometry is a **research shadow proxy**, not a selected/calibrated General Metric or production-eligible signal.

See [integration contract](docs/integration-contract.md) for ownership, prerequisites, negative tests and explicit release gates.

## Relationship with existing providers

The name follows `bliss-guidance-lastfm` and `bliss-guidance-library-signals`. The `bliss-` prefix refers to the **SPI/host integration**, not a dependency of the Essentia similarity engine itself. Use the future `lms-guidance-essentia` adapter for UI/configuration/discovery, as in those two precedents.
