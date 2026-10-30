// SPDX-License-Identifier: GPL-3.0-only
//! Native SPI v2 process, deliberately shadow-neutral until the actual
//! General Metric and human listening-quality gate are independently passed.
//! The ONLY emitted guidance list is empty. No playlist or sidecar writes.

use bliss_playlist_guidance_spi::{
    decode_request, encode, Anchor, ArtifactDescriptor, Candidate, Capability, ChannelDescriptor,
    Diagnostics, GuidanceRequest, GuidanceResponse, GuidanceScope, Manifest, ResourceAccess,
    ResourceDescriptor, ScoreContext, PROTOCOL_NAME, SPI_VERSION,
};
use essentia_feature_store::inspect;
use essentia_similarity_core::{geometric_similarity, Metric};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, BufRead, Read, Write};
use std::path::PathBuf;

const PROVIDER: &str = "essentia-guidance";
const IDENTITY_KIND: &str = "essentia-exact-track-keys-v1";
const RESOURCE_KIND: &str = "essentia-feature-sqlite-v3";
const MAX_ARTIFACT_BYTES: u64 = 256 * 1024;
const MAX_IDENTITIES: usize = 512;
const MAX_CANDIDATES: usize = 50;
const MAX_LINE_BYTES: usize = 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExactKeyArtifact {
    schema: String,
    schema_version: u8,
    job_id: String,
    tracks: Vec<ExactKey>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExactKey {
    track_id: String,
    track_file: String,
}

struct Prepared {
    database: PathBuf,
    model_id: String,
    metric: Metric,
    expected_model_key: String,
    experimental_preview_opt_in: bool,
    keys: HashMap<String, String>,
    anchors: HashSet<String>,
    requests: u64,
}

#[derive(Default)]
struct Provider {
    state: Option<Prepared>,
}

fn diagnostics(state: &str, reason: Option<&str>, count: usize, requests: u64) -> Diagnostics {
    Diagnostics {
        state: Some(state.to_owned()),
        request_count: requests,
        failure_count: u64::from(reason.is_some()),
        details: Some(json!({
            "mode": "shadow_only",
            "provider_active_guidance": false,
            "candidate_count": count,
            "reason": reason,
            "playlist_queue_or_sidecar_writes": false,
            "actual_general_quality_validated": false
        })),
    }
}

fn manifest() -> Manifest {
    Manifest {
        spi_version: SPI_VERSION,
        provider_id: PROVIDER.into(),
        provider_version: env!("CARGO_PKG_VERSION").into(),
        protocol: PROTOCOL_NAME.into(),
        capabilities: vec![Capability::GlobalCandidateGuidance],
        channels: vec![ChannelDescriptor {
            channel: "essentia_experimental_shadow".into(),
            scopes: vec![GuidanceScope::Global],
            // No active weights/policies until independently validated.
            supported_host_policies: vec![],
        }],
        required_context: vec!["candidate_identity".into()],
        configuration_schema: Some(json!({
            "type":"object",
            "properties":{
                "model_id":{"type":"string","enum":["genre","moodtheme"]},
                "metric":{"type":"string","enum":["affine_cosine_v1","affine_centered_cosine_v1"]},
                "expected_model_key":{"type":"string","minLength":1},
                "experimental_preview_opt_in":{"type":"boolean"}
            },
            "required":["model_id","metric","expected_model_key"],
            "additionalProperties":false
        })),
    }
}

fn error(code: &str) -> GuidanceResponse {
    GuidanceResponse::Error {
        provider_id: Some(PROVIDER.into()),
        code: code.into(),
        // No user input, paths, candidate IDs or JSON text in error messages.
        message: code.into(),
        retryable: false,
    }
}

fn one_artifact<'a>(
    artifacts: &'a [ArtifactDescriptor],
) -> Result<&'a ArtifactDescriptor, &'static str> {
    if artifacts.len() != 1 || artifacts[0].kind != IDENTITY_KIND {
        return Err("EXPECTED_ONE_PINNED_IDENTITY_ARTIFACT");
    }
    Ok(&artifacts[0])
}

fn one_resource<'a>(
    resources: &'a [ResourceDescriptor],
) -> Result<&'a ResourceDescriptor, &'static str> {
    if resources.len() != 1
        || resources[0].kind != RESOURCE_KIND
        || resources[0].access != ResourceAccess::ReadOnly
        || !PathBuf::from(&resources[0].path).is_absolute()
    {
        return Err("EXPECTED_ONE_TRUSTED_READ_ONLY_SIDECAR");
    }
    Ok(&resources[0])
}

fn verified_artifact(artifact: &ArtifactDescriptor) -> Result<ExactKeyArtifact, &'static str> {
    if artifact.sha256.len() != 64
        || !artifact
            .sha256
            .bytes()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
    {
        return Err("INVALID_ARTIFACT_FINGERPRINT");
    }
    let metadata = fs::metadata(&artifact.path).map_err(|_| "ARTIFACT_UNAVAILABLE")?;
    if !metadata.is_file() || metadata.len() > MAX_ARTIFACT_BYTES {
        return Err("ARTIFACT_TOO_LARGE_OR_INVALID");
    }
    // Read through a hard cap even if the file changes after metadata() (TOCTOU).
    let file = fs::File::open(&artifact.path).map_err(|_| "ARTIFACT_UNAVAILABLE")?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_ARTIFACT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "ARTIFACT_UNAVAILABLE")?;
    if bytes.len() as u64 > MAX_ARTIFACT_BYTES {
        return Err("ARTIFACT_TOO_LARGE_OR_INVALID");
    }
    if format!("{:x}", Sha256::digest(&bytes)) != artifact.sha256 {
        return Err("ARTIFACT_SHA256_MISMATCH");
    }
    serde_json::from_slice(&bytes).map_err(|_| "INVALID_EXACT_KEY_ARTIFACT")
}

fn prepare(
    job_id: String,
    options: Value,
    artifacts: Vec<ArtifactDescriptor>,
    resources: Vec<ResourceDescriptor>,
    anchors: Vec<Anchor>,
) -> Result<Prepared, &'static str> {
    let options = options.as_object().ok_or("INVALID_PREPARE_OPTIONS")?;
    if !options.keys().all(|k| {
        matches!(
            k.as_str(),
            "model_id" | "metric" | "expected_model_key" | "experimental_preview_opt_in"
        )
    }) {
        return Err("UNSUPPORTED_PREPARE_OPTION");
    }
    let model_id = options
        .get("model_id")
        .and_then(Value::as_str)
        .ok_or("MISSING_MODEL_ID")?;
    if !matches!(model_id, "genre" | "moodtheme") {
        return Err("UNSUPPORTED_MODEL_ID");
    }
    let metric = options
        .get("metric")
        .and_then(Value::as_str)
        .ok_or("MISSING_METRIC")
        .and_then(Metric::parse)?;
    let key = options
        .get("expected_model_key")
        .and_then(Value::as_str)
        .filter(|k| !k.is_empty() && k.len() <= 128)
        .ok_or("INVALID_MODEL_FINGERPRINT")?;
    let optin = match options.get("experimental_preview_opt_in") {
        Some(value) => value.as_bool().ok_or("INVALID_EXPERIMENTAL_OPT_IN")?,
        None => false,
    };
    let resource = one_resource(&resources)?;
    let artifact = verified_artifact(one_artifact(&artifacts)?)?;
    if artifact.schema != IDENTITY_KIND
        || artifact.schema_version != 1
        || artifact.job_id != job_id
        || !(1..=MAX_IDENTITIES).contains(&artifact.tracks.len())
        || anchors.is_empty()
        || anchors.len() > MAX_IDENTITIES
    {
        return Err("UNTRUSTED_OR_INCOMPLETE_IDENTITY_COHORT");
    }
    let mut keys = HashMap::new();
    let mut feature_keys = HashSet::new();
    for row in artifact.tracks {
        if row.track_id.is_empty()
            || row.track_file.is_empty()
            || row.track_id.contains('\0')
            || row.track_file.contains('\0')
            || !feature_keys.insert(row.track_file.clone())
            || keys.insert(row.track_id, row.track_file).is_some()
        {
            return Err("DUPLICATE_OR_INVALID_TRACK_IDENTITY");
        }
    }
    let mut anchor_ids = HashSet::new();
    for anchor in anchors {
        let exact_key = keys.get(&anchor.anchor_id);
        if anchor.anchor_id.is_empty()
            || exact_key.is_none()
            || anchor
                .track
                .database_file
                .as_ref()
                .is_some_and(|host_key| Some(host_key) != exact_key)
            || !anchor_ids.insert(anchor.anchor_id)
        {
            return Err("ANCHOR_EXACT_KEY_UNRESOLVED");
        }
    }
    let database = PathBuf::from(&resource.path);
    // Checks actual scanner-compatible v3 schema and read-only opening.
    // Missing feature rows may be reported at score time with whole-query fallback.
    let sample_key = keys
        .values()
        .next()
        .ok_or("EMPTY_EXACT_KEY_ARTIFACT")?
        .clone();
    inspect(&database, &[sample_key], model_id, optin)
        .map_err(|_| "FEATURE_SIDECAR_SCHEMA_OR_READ_ONLY_OPEN_FAILED")?;
    Ok(Prepared {
        database,
        model_id: model_id.into(),
        metric,
        expected_model_key: key.into(),
        experimental_preview_opt_in: optin,
        keys,
        anchors: anchor_ids,
        requests: 0,
    })
}

fn neutral(request_id: String, reason: &str, count: usize, requests: u64) -> GuidanceResponse {
    GuidanceResponse::Scores {
        provider_id: PROVIDER.into(),
        request_id,
        signals: vec![],
        diagnostics: diagnostics("neutral_fallback", Some(reason), count, requests),
    }
}

fn score(
    prepared: &mut Prepared,
    request_id: String,
    context: ScoreContext,
    candidates: Vec<Candidate>,
) -> GuidanceResponse {
    prepared.requests += 1;
    let requests = prepared.requests;
    let n = candidates.len();
    if context.scope != GuidanceScope::Global {
        return neutral(
            request_id,
            "EDGE_DIRECTIONAL_MODEL_NOT_VALIDATED",
            n,
            requests,
        );
    }
    if context.context_track_ids.len() != 1
        || !prepared.anchors.contains(&context.context_track_ids[0])
        || context.left_anchor_id.is_some()
        || context.right_anchor_id.is_some()
    {
        return neutral(
            request_id,
            "ONE_VERIFIED_GLOBAL_ANCHOR_REQUIRED",
            n,
            requests,
        );
    }
    if !(2..=MAX_CANDIDATES).contains(&n) {
        return neutral(request_id, "BOUNDED_CANDIDATE_BATCH_REQUIRED", n, requests);
    }
    let mut seen = HashSet::new();
    let mut track_keys = Vec::with_capacity(n + 1);
    let anchor_key = &prepared.keys[&context.context_track_ids[0]];
    track_keys.push(anchor_key.clone());
    for candidate in &candidates {
        if candidate.candidate_id.is_empty() || !seen.insert(&candidate.candidate_id) {
            return neutral(request_id, "DUPLICATE_OR_EMPTY_CANDIDATE_ID", n, requests);
        }
        let Some(key) = prepared.keys.get(&candidate.candidate_id) else {
            return neutral(request_id, "HOST_EXACT_FILE_IDENTITY_MISSING", n, requests);
        };
        if candidate
            .database_file
            .as_ref()
            .is_some_and(|host_key| host_key != key)
        {
            return neutral(request_id, "HOST_IDENTITY_ARTIFACT_MISMATCH", n, requests);
        }
        track_keys.push(key.clone());
    }
    let rows = match inspect(
        &prepared.database,
        &track_keys,
        &prepared.model_id,
        prepared.experimental_preview_opt_in,
    ) {
        Ok(rows) if rows.len() == track_keys.len() => rows,
        _ => {
            return neutral(
                request_id,
                "SIDECAR_UNAVAILABLE_OR_SCHEMA_INVALID",
                n,
                requests,
            )
        }
    };
    let mut labels: Option<&[String]> = None;
    let mut vectors = Vec::with_capacity(rows.len());
    for result in &rows {
        let Some(model) = result.models.get(&prepared.model_id) else {
            return neutral(request_id, "HEAD_NOT_FOUND", n, requests);
        };
        if !matches!(model.status, "validated" | "experimental") {
            return neutral(
                request_id,
                "SOURCE_FEATURES_MISSING_INVALID_OR_UNVALIDATED",
                n,
                requests,
            );
        }
        if model.status == "experimental" && !prepared.experimental_preview_opt_in {
            return neutral(
                request_id,
                "SOURCE_REFERENCE_PARITY_UNVALIDATED",
                n,
                requests,
            );
        }
        if model.model_key.as_deref() != Some(&prepared.expected_model_key) {
            return neutral(request_id, "MODEL_FINGERPRINT_MISMATCH", n, requests);
        }
        let (Some(vector), Some(class_names)) = (&model.scores_f32, &model.labels) else {
            return neutral(request_id, "FEATURE_VECTOR_UNAVAILABLE", n, requests);
        };
        if labels.is_some_and(|expected| expected != class_names.as_slice()) {
            return neutral(request_id, "LABEL_ORDER_INCONSISTENT", n, requests);
        }
        labels = Some(class_names);
        vectors.push(vector);
    }
    for values in vectors.iter().skip(1) {
        if geometric_similarity(vectors[0], values, prepared.metric).is_err() {
            return neutral(request_id, "SCORE_GEOMETRY_UNUSABLE", n, requests);
        }
    }
    // Real feature-vectors have been evaluated. Still NO guidance scores:
    // geometry is not perceptually calibrated, no General Metric winner exists.
    GuidanceResponse::Scores {
        provider_id: PROVIDER.into(),
        request_id,
        signals: vec![],
        diagnostics: diagnostics("shadow_evaluated_no_active_signals", None, n, requests),
    }
}

fn handle(provider: &mut Provider, request: GuidanceRequest) -> GuidanceResponse {
    match request {
        GuidanceRequest::Describe { spi_version } => {
            if spi_version == SPI_VERSION {
                GuidanceResponse::Manifest(manifest())
            } else {
                error("UNSUPPORTED_SPI_VERSION")
            }
        }
        GuidanceRequest::Prepare {
            spi_version,
            job_id,
            options,
            artifacts,
            resources,
            anchors,
        } => {
            if spi_version != SPI_VERSION {
                return error("UNSUPPORTED_SPI_VERSION");
            }
            provider.state = None;
            match prepare(job_id, options, artifacts, resources, anchors) {
                Ok(state) => {
                    provider.state = Some(state);
                    GuidanceResponse::Prepared {
                        provider_id: PROVIDER.into(),
                        snapshot_id: None,
                        diagnostics: diagnostics("prepared_shadow_only", None, 0, 0),
                    }
                }
                Err(code) => error(code),
            }
        }
        GuidanceRequest::Score {
            spi_version,
            request_id,
            context,
            candidates,
        } => {
            if spi_version != SPI_VERSION {
                return error("UNSUPPORTED_SPI_VERSION");
            }
            match provider.state.as_mut() {
                Some(state) => score(state, request_id, context, candidates),
                None => neutral(request_id, "NO_PREPARED_SNAPSHOT", candidates.len(), 0),
            }
        }
        GuidanceRequest::Close { spi_version } => {
            if spi_version != SPI_VERSION {
                return error("UNSUPPORTED_SPI_VERSION");
            }
            provider.state = None;
            GuidanceResponse::Closed {
                provider_id: PROVIDER.into(),
            }
        }
    }
}

fn run() -> io::Result<()> {
    let mut provider = Provider::default();
    let stdin = io::stdin();
    let mut out = io::BufWriter::new(io::stdout().lock());
    // Prevent an arbitrarily large line from being accepted as provider input.
    let mut reader = io::BufReader::new(stdin.lock());
    loop {
        let mut buf = Vec::new();
        let limit = (MAX_LINE_BYTES + 1) as u64;
        let read = reader.by_ref().take(limit).read_until(b'\n', &mut buf)?;
        if read == 0 {
            break;
        }
        if buf.len() > MAX_LINE_BYTES || !buf.ends_with(b"\n") {
            writeln!(
                out,
                "{}",
                encode(&error("INPUT_LINE_TOO_LARGE_OR_UNTERMINATED")).unwrap()
            )?;
            out.flush()?;
            break;
        }
        if buf.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        let response = match std::str::from_utf8(&buf)
            .ok()
            .and_then(|line| decode_request(line).ok())
        {
            Some(request) => handle(&mut provider, request),
            None => error("INVALID_SPI_REQUEST"),
        };
        writeln!(out, "{}", encode(&response).unwrap())?;
        out.flush()?;
        if matches!(response, GuidanceResponse::Closed { .. }) {
            break;
        }
    }
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.as_slice() == ["version"] {
        println!("bliss-guidance-essentia {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if args.as_slice() == ["version", "--json"] {
        println!(
            "{}",
            json!({
                "schema_version":1,"program":"bliss-guidance-essentia",
                "version":env!("CARGO_PKG_VERSION"),"provider_id":PROVIDER,
                "spi_version":SPI_VERSION,"mode":"shadow_only"
            })
        );
        return;
    }
    if !args.is_empty() {
        eprintln!("Usage: bliss-guidance-essentia [version [--json]]");
        std::process::exit(2);
    }
    if run().is_err() {
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bliss_playlist_guidance_spi::{Anchor, GuidanceScope, ResourceAccess};
    use rusqlite::{params, Connection};
    use std::time::{SystemTime, UNIX_EPOCH};

    struct Fixture {
        directory: PathBuf,
        resource: ResourceDescriptor,
        artifact: ArtifactDescriptor,
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.directory);
        }
    }

    fn candidate(id: &str) -> Candidate {
        Candidate {
            candidate_id: id.into(),
            lms_urlmd5: None,
            database_file: None,
            title: None,
            artist: None,
            album: None,
            recording_mbid: None,
            artist_mbids: vec![],
        }
    }
    fn fixture(validated: i64) -> Fixture {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "essentia-spi-fixture-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir(&directory).unwrap();
        let db = directory.join("sidecar.db");
        let conn = Connection::open(&db).unwrap();
        conn.execute_batch(
            "PRAGMA user_version=3;
             CREATE TABLE feature_models(
                model_key TEXT PRIMARY KEY,
                preprocessing_version TEXT, aggregation TEXT, classes_json TEXT,
                validated_against_essentia INTEGER
             );
             CREATE TABLE feature_tracks(
                track_file TEXT, model_id TEXT, capability TEXT, model_key TEXT,
                scores_f32_le BLOB, PRIMARY KEY(track_file,model_id)
             );",
        )
        .unwrap();
        let names = (0..87).map(|n| format!("class{n}")).collect::<Vec<_>>();
        conn.execute(
            "INSERT INTO feature_models VALUES (?1,?2,?3,?4,?5)",
            params![
                "fake-pinned-key",
                "preprocess-v1",
                "mean",
                serde_json::to_string(&names).unwrap(),
                validated
            ],
        )
        .unwrap();
        for (name, shift) in [("seed", 0), ("one", 10), ("two", 23)] {
            let vals: Vec<f32> = (0..87)
                .map(|i| 0.05 + (((i + shift) * 31) % 83) as f32 / 100.0)
                .collect();
            let bytes: Vec<u8> = vals.iter().flat_map(|v| v.to_le_bytes()).collect();
            conn.execute(
                "INSERT INTO feature_tracks VALUES (?1,'genre','genre',?2,?3)",
                params![format!("fictional/{name}.flac"), "fake-pinned-key", bytes],
            )
            .unwrap();
        }
        drop(conn);
        let job_id = "fictional-job";
        let data = json!({
            "schema": IDENTITY_KIND, "schema_version":1,"job_id":job_id,
            "tracks":[
                {"track_id":"seed","track_file":"fictional/seed.flac"},
                {"track_id":"one","track_file":"fictional/one.flac"},
                {"track_id":"two","track_file":"fictional/two.flac"}
            ]
        });
        let bytes = serde_json::to_vec(&data).unwrap();
        let artifact_path = directory.join("identities.json");
        fs::write(&artifact_path, &bytes).unwrap();
        Fixture {
            directory,
            resource: ResourceDescriptor {
                kind: RESOURCE_KIND.into(),
                path: db.to_string_lossy().into_owned(),
                access: ResourceAccess::ReadOnly,
            },
            artifact: ArtifactDescriptor {
                kind: IDENTITY_KIND.into(),
                path: artifact_path.to_string_lossy().into_owned(),
                sha256: format!("{:x}", Sha256::digest(&bytes)),
            },
        }
    }
    fn prep(f: &Fixture, optin: bool) -> GuidanceRequest {
        GuidanceRequest::Prepare {
            spi_version: SPI_VERSION,
            job_id: "fictional-job".into(),
            options: json!({"model_id":"genre","metric":"affine_centered_cosine_v1",
                        "expected_model_key":"fake-pinned-key","experimental_preview_opt_in":optin}),
            artifacts: vec![f.artifact.clone()],
            resources: vec![f.resource.clone()],
            anchors: vec![Anchor {
                anchor_id: "seed".into(),
                track: candidate("seed"),
            }],
        }
    }
    fn req() -> GuidanceRequest {
        GuidanceRequest::Score {
            spi_version: SPI_VERSION,
            request_id: "fake-query".into(),
            context: ScoreContext {
                scope: GuidanceScope::Global,
                left_anchor_id: None,
                right_anchor_id: None,
                context_track_ids: vec!["seed".into()],
            },
            candidates: vec![candidate("one"), candidate("two")],
        }
    }
    fn score_info(r: GuidanceResponse) -> (usize, String) {
        if let GuidanceResponse::Scores {
            signals,
            diagnostics,
            ..
        } = r
        {
            (signals.len(), diagnostics.state.unwrap())
        } else {
            panic!("expected Scores")
        }
    }

    #[test]
    fn canonical_manifest_never_advertises_active_policy() {
        let r = handle(
            &mut Provider::default(),
            GuidanceRequest::Describe { spi_version: 2 },
        );
        let GuidanceResponse::Manifest(m) = r else {
            panic!("manifest")
        };
        assert_eq!(m.protocol, PROTOCOL_NAME);
        assert_eq!(m.provider_id, PROVIDER);
        assert_eq!(m.channels.len(), 1);
        assert!(m.channels[0].supported_host_policies.is_empty());
        assert_eq!(m.channels[0].scopes, vec![GuidanceScope::Global]);
    }
    #[test]
    fn correct_jsonl_protocol_roundtrip() {
        let r = GuidanceRequest::Describe {
            spi_version: SPI_VERSION,
        };
        let decoded = decode_request(&encode(&r).unwrap()).unwrap();
        assert!(matches!(
            decoded,
            GuidanceRequest::Describe { spi_version: 2 }
        ));
    }
    #[test]
    fn full_validated_sqlite_v3_shadow_is_real_but_always_neutral_to_host() {
        let f = fixture(1);
        let mut p = Provider::default();
        assert!(matches!(
            handle(&mut p, prep(&f, false)),
            GuidanceResponse::Prepared { .. }
        ));
        let (count, status) = score_info(handle(&mut p, req()));
        assert_eq!(count, 0);
        assert_eq!(status, "shadow_evaluated_no_active_signals");
    }
    #[test]
    fn missing_prepare_is_neutral_and_close_releases_state() {
        let mut p = Provider::default();
        let (_, s) = score_info(handle(&mut p, req()));
        assert_eq!(s, "neutral_fallback");
        let f = fixture(1);
        handle(&mut p, prep(&f, false));
        assert!(matches!(
            handle(&mut p, GuidanceRequest::Close { spi_version: 2 }),
            GuidanceResponse::Closed { .. }
        ));
        assert!(p.state.is_none());
    }
    #[test]
    fn wrong_artifact_sha_is_rejected_without_path_or_track_leakage() {
        let mut f = fixture(1);
        f.artifact.sha256 = "a".repeat(64);
        let r = handle(&mut Provider::default(), prep(&f, false));
        if let GuidanceResponse::Error { code, message, .. } = r {
            assert_eq!(code, "ARTIFACT_SHA256_MISMATCH");
            assert_eq!(message, code);
            assert!(!message.contains("fictional"));
        } else {
            panic!("must error")
        }
    }
    #[test]
    fn unvalidated_inputs_are_neutral_without_opt_in_and_shadow_only_with_it() {
        let f = fixture(0);
        let mut p = Provider::default();
        handle(&mut p, prep(&f, false));
        assert_eq!(score_info(handle(&mut p, req())).1, "neutral_fallback");
        handle(&mut p, prep(&f, true));
        assert_eq!(
            score_info(handle(&mut p, req())).1,
            "shadow_evaluated_no_active_signals"
        );
    }
    #[test]
    fn missing_candidate_and_conflicting_sidecar_key_fallback_full_batch() {
        let f = fixture(1);
        let mut p = Provider::default();
        handle(&mut p, prep(&f, false));
        let mut q = req();
        if let GuidanceRequest::Score { candidates, .. } = &mut q {
            candidates[1].candidate_id = "not-in-artifact".into();
        }
        assert_eq!(score_info(handle(&mut p, q)).1, "neutral_fallback");
        let mut q = req();
        if let GuidanceRequest::Score { candidates, .. } = &mut q {
            candidates[0].database_file = Some("wrong-key".into());
        }
        assert_eq!(score_info(handle(&mut p, q)).1, "neutral_fallback");
    }
    #[test]
    fn spoofed_edge_and_wrong_host_anchor_never_emit_signals() {
        let f = fixture(1);
        let mut p = Provider::default();
        handle(&mut p, prep(&f, false));
        let mut q = req();
        if let GuidanceRequest::Score { context, .. } = &mut q {
            context.scope = GuidanceScope::Edge;
        }
        assert_eq!(score_info(handle(&mut p, q)).1, "neutral_fallback");
        let mut q = req();
        if let GuidanceRequest::Score { context, .. } = &mut q {
            context.context_track_ids = vec!["one".into()];
        }
        assert_eq!(score_info(handle(&mut p, q)).1, "neutral_fallback");
    }
    #[test]
    fn unsupported_spi_and_untrusted_resource_are_rejected() {
        let mut p = Provider::default();
        assert!(matches!(
            handle(&mut p, GuidanceRequest::Describe { spi_version: 1 }),
            GuidanceResponse::Error { .. }
        ));
        let mut f = fixture(1);
        f.resource.access = ResourceAccess::ReadOnly;
        f.resource.kind = "other-db".into();
        assert!(matches!(
            handle(&mut p, prep(&f, false)),
            GuidanceResponse::Error { .. }
        ));
    }
}
