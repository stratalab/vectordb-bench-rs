//! Pure-serde round-trip tests for the wire types.
//!
//! These tests do not require a running strata binary. They verify that each
//! `Command` / `Output` variant we mirror encodes and decodes through
//! `rmp_serde::to_vec_named` round-trip, catching any accidental field-name
//! typo at build time.

use vbench_strata_ipc::{
    BatchVectorEntry, Command, DistanceMetric, IpcError, Output, Request, Response, VectorMatch,
};

fn round_trip<T: serde::Serialize + serde::de::DeserializeOwned>(value: &T) -> T {
    let bytes = rmp_serde::to_vec_named(value).expect("encode");
    rmp_serde::from_slice::<T>(&bytes).expect("decode")
}

#[test]
fn ping_request_round_trip() {
    let req = Request {
        id: 42,
        command: Command::Ping,
    };
    let decoded = round_trip(&req);
    assert_eq!(decoded.id, 42);
    assert!(matches!(decoded.command, Command::Ping));
}

#[test]
fn vector_create_collection_round_trip() {
    let req = Request {
        id: 7,
        command: Command::VectorCreateCollection {
            branch: None,
            space: None,
            collection: "vbench".to_string(),
            dimension: 768,
            metric: DistanceMetric::Cosine,
        },
    };
    let decoded = round_trip(&req);
    match decoded.command {
        Command::VectorCreateCollection {
            collection,
            dimension,
            metric,
            branch,
            space,
        } => {
            assert_eq!(collection, "vbench");
            assert_eq!(dimension, 768);
            assert_eq!(metric, DistanceMetric::Cosine);
            assert!(branch.is_none());
            assert!(space.is_none());
        }
        other => panic!("wrong variant: {other:?}"),
    }
}

#[test]
fn vector_batch_upsert_round_trip() {
    let req = Request {
        id: 1,
        command: Command::VectorBatchUpsert {
            branch: None,
            space: None,
            collection: "vbench".to_string(),
            entries: vec![
                BatchVectorEntry {
                    key: "0".to_string(),
                    vector: vec![0.1, 0.2, 0.3],
                    metadata: None,
                },
                BatchVectorEntry {
                    key: "1".to_string(),
                    vector: vec![0.4, 0.5, 0.6],
                    metadata: None,
                },
            ],
        },
    };
    let decoded = round_trip(&req);
    match decoded.command {
        Command::VectorBatchUpsert {
            entries,
            collection,
            ..
        } => {
            assert_eq!(collection, "vbench");
            assert_eq!(entries.len(), 2);
            assert_eq!(entries[0].key, "0");
            assert_eq!(entries[0].vector, vec![0.1, 0.2, 0.3]);
            assert_eq!(entries[1].key, "1");
        }
        other => panic!("wrong variant: {other:?}"),
    }
}

#[test]
fn vector_query_round_trip() {
    let req = Request {
        id: 99,
        command: Command::VectorQuery {
            branch: None,
            space: None,
            collection: "vbench".to_string(),
            query: vec![0.1; 768],
            k: 10,
            filter: None,
            metric: None,
            as_of: None,
        },
    };
    let decoded = round_trip(&req);
    match decoded.command {
        Command::VectorQuery {
            query, k, filter, ..
        } => {
            assert_eq!(query.len(), 768);
            assert_eq!(k, 10);
            assert!(filter.is_none());
        }
        other => panic!("wrong variant: {other:?}"),
    }
}

#[test]
fn pong_response_round_trip() {
    let resp = Response {
        id: 1,
        result: Ok(Output::Pong {
            version: "0.6.1".to_string(),
        }),
    };
    let decoded = round_trip(&resp);
    match decoded.result.unwrap() {
        Output::Pong { version } => assert_eq!(version, "0.6.1"),
        other => panic!("wrong variant: {other:?}"),
    }
}

#[test]
fn version_response_round_trip() {
    let resp = Response {
        id: 1,
        result: Ok(Output::Version(42)),
    };
    let decoded = round_trip(&resp);
    match decoded.result.unwrap() {
        Output::Version(v) => assert_eq!(v, 42),
        other => panic!("wrong variant: {other:?}"),
    }
}

#[test]
fn versions_response_round_trip() {
    let resp = Response {
        id: 1,
        result: Ok(Output::Versions(vec![1, 2, 3])),
    };
    let decoded = round_trip(&resp);
    match decoded.result.unwrap() {
        Output::Versions(vs) => assert_eq!(vs, vec![1, 2, 3]),
        other => panic!("wrong variant: {other:?}"),
    }
}

#[test]
fn vector_matches_response_round_trip() {
    let resp = Response {
        id: 1,
        result: Ok(Output::VectorMatches(vec![
            VectorMatch {
                key: "0".to_string(),
                score: 0.99,
                metadata: None,
            },
            VectorMatch {
                key: "1".to_string(),
                score: 0.88,
                metadata: None,
            },
        ])),
    };
    let decoded = round_trip(&resp);
    match decoded.result.unwrap() {
        Output::VectorMatches(m) => {
            assert_eq!(m.len(), 2);
            assert_eq!(m[0].key, "0");
            assert!((m[0].score - 0.99).abs() < 1e-5);
        }
        other => panic!("wrong variant: {other:?}"),
    }
}

#[test]
fn bool_response_round_trip() {
    let resp = Response {
        id: 1,
        result: Ok(Output::Bool(true)),
    };
    let decoded = round_trip(&resp);
    match decoded.result.unwrap() {
        Output::Bool(b) => assert!(b),
        other => panic!("wrong variant: {other:?}"),
    }
}

#[test]
fn error_response_round_trip() {
    // Simulate a strata-shaped error: externally-tagged variant with a
    // structured body. Our IpcError captures it opaquely.
    let resp = Response {
        id: 1,
        result: Err(IpcError(rmpv::Value::Map(vec![(
            rmpv::Value::String("CollectionNotFound".into()),
            rmpv::Value::Map(vec![(
                rmpv::Value::String("collection".into()),
                rmpv::Value::String("vbench".into()),
            )]),
        )]))),
    };
    let decoded = round_trip(&resp);
    let err = decoded.result.unwrap_err();
    let display = err.to_string();
    assert!(
        display.contains("CollectionNotFound"),
        "display should surface the variant name, got {display}"
    );
}

#[test]
fn snake_case_metric_encoding() {
    // DistanceMetric::DotProduct must serialise as the string "dot_product"
    // (matching strata-core's #[serde(rename_all = "snake_case")]).
    let metric = DistanceMetric::DotProduct;
    let bytes = rmp_serde::to_vec_named(&metric).unwrap();
    // MessagePack fixstr for "dot_product": 0xab (fixstr len 11) + 11 bytes
    assert_eq!(bytes[0], 0xab);
    assert_eq!(&bytes[1..], b"dot_product");
}
