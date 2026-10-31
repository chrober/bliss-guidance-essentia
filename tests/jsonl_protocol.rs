// SPDX-License-Identifier: GPL-3.0-only
//! End-to-end native JSONL process tests (not only an in-process handler).
use bliss_playlist_guidance_spi::{decode_response, GuidanceResponse, PROTOCOL_NAME, SPI_VERSION};
use std::io::Write;
use std::process::{Command, Output, Stdio};

fn exchange(lines: &[&str]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_bliss-guidance-essentia"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("native executable should spawn");
    {
        let mut stdin = child.stdin.take().unwrap();
        for line in lines {
            writeln!(stdin, "{line}").unwrap();
        }
    }
    child
        .wait_with_output()
        .expect("native process should exit")
}

#[test]
fn actual_describe_and_close_are_wire_compatible() {
    let result = exchange(&[
        r#"{"type":"describe","spi_version":2}"#,
        r#"{"type":"close","spi_version":2}"#,
    ]);
    assert!(result.status.success());
    assert!(result.stderr.is_empty());
    let stdout = String::from_utf8(result.stdout).unwrap();
    let replies = stdout
        .lines()
        .map(|line| decode_response(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(replies.len(), 2);
    match &replies[0] {
        GuidanceResponse::Manifest(manifest) => {
            assert_eq!(manifest.spi_version, SPI_VERSION);
            assert_eq!(manifest.protocol, PROTOCOL_NAME);
            assert_eq!(manifest.provider_id, "essentia-guidance");
            assert!(manifest
                .channels
                .iter()
                .all(|channel| channel.supported_host_policies.is_empty()));
        }
        other => panic!("expected real Manifest, got {other:?}"),
    }
    assert!(
        matches!(&replies[1], GuidanceResponse::Closed{provider_id} if provider_id=="essentia-guidance")
    );
}

#[test]
fn actual_unprepared_score_is_neutral_without_candidate_leak() {
    let result = exchange(&[
        r#"{"type":"score","spi_version":2,"request_id":"fixture-request","context":{"scope":"global","context_track_ids":["fake-anchor"]},"candidates":[{"candidate_id":"fixture-hidden-song-1"},{"candidate_id":"fixture-hidden-song-2"}]}"#,
        r#"{"type":"close","spi_version":2}"#,
    ]);
    assert!(result.status.success());
    let stdout = String::from_utf8(result.stdout).unwrap();
    assert!(!stdout.contains("fixture-hidden-song"));
    let first = decode_response(stdout.lines().next().unwrap()).unwrap();
    match first {
        GuidanceResponse::Scores {
            signals,
            request_id,
            diagnostics,
            ..
        } => {
            assert!(signals.is_empty());
            assert_eq!(request_id, "fixture-request");
            assert_eq!(diagnostics.state.as_deref(), Some("neutral_fallback"));
        }
        other => panic!("expected neutral Scores, got {other:?}"),
    }
}

#[test]
fn malformed_line_is_bounded_error_and_following_describe_still_works() {
    let result = exchange(&[
        r#"{"type":"prepare","spi_version":2,"random_private_path":"/fake/not-logged"}"#,
        r#"{"type":"describe","spi_version":2}"#,
        r#"{"type":"close","spi_version":2}"#,
    ]);
    assert!(result.status.success());
    let stdout = String::from_utf8(result.stdout).unwrap();
    assert!(!stdout.contains("/fake/not-logged"));
    let replies = stdout
        .lines()
        .map(|line| decode_response(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(replies.len(), 3);
    assert!(matches!(&replies[0], GuidanceResponse::Error { .. }));
    assert!(matches!(&replies[1], GuidanceResponse::Manifest(_)));
    assert!(matches!(&replies[2], GuidanceResponse::Closed { .. }));
}
