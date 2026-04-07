//! Field-name guard for the `TestResult` JSON schema.
//!
//! Serialises a synthetic `TestResult` and asserts every key name matches
//! VectorDBBench's `vectordb_bench/models.py:TestResult`. If anyone renames
//! a field on either side this test fails loudly, instead of producing
//! silently-incomparable JSON.
//!
//! When upstream's schema evolves, the fix is to update both this expected
//! list and the corresponding `result.rs` field name in the same commit.

use std::collections::BTreeSet;

use vbench_core::{
    CaseConfig, DbConfig, HostInfo, ResultMetrics, TaskConfig, TestResult, Timestamps,
};

fn synthetic_result() -> TestResult {
    TestResult {
        vbench_schema_version: "1".to_string(),
        task_label: "test".to_string(),
        db_config: DbConfig {
            adapter: "test".to_string(),
            db_version: "0.0.0".to_string(),
            install_method: Some("test".to_string()),
            hnsw_m: Some("default".to_string()),
            hnsw_ef_construction: Some("default".to_string()),
            hnsw_ef_search: Some("default".to_string()),
            notes: Some("synthetic".to_string()),
        },
        case_config: CaseConfig {
            dataset: "synthetic".to_string(),
            dim: 8,
            metric: "cosine".to_string(),
            recall_k: 10,
            num_train: 100,
            num_test: 10,
        },
        task_config: TaskConfig {
            batch_size: 1000,
            warmup_queries: 200,
            run_concurrent: false,
        },
        metrics: ResultMetrics {
            load_duration: 1.5,
            optimize_duration: 0.5,
            recall: 0.95,
            ndcg: 0.92,
            serial_latency_avg: 1.2,
            serial_latency_p50: 1.0,
            serial_latency_p95: 2.5,
            serial_latency_p99: 3.0,
            serial_query_count: 10,
            conc_qps_list: vec![],
            conc_latency_p99_list: vec![],
        },
        timestamps: Timestamps {
            started_at: "2026-04-07T00:00:00Z".to_string(),
            finished_at: "2026-04-07T00:00:02Z".to_string(),
            host: HostInfo::snapshot(),
        },
    }
}

fn collect_keys_recursive(value: &serde_json::Value, prefix: &str, out: &mut BTreeSet<String>) {
    match value {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                let path = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                out.insert(path.clone());
                collect_keys_recursive(v, &path, out);
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr {
                collect_keys_recursive(v, prefix, out);
            }
        }
        _ => {}
    }
}

#[test]
fn schema_emits_expected_top_level_keys() {
    let result = synthetic_result();
    let json = serde_json::to_value(&result).unwrap();
    let obj = json.as_object().expect("top-level is an object");

    let keys: BTreeSet<String> = obj.keys().cloned().collect();

    // The set of fields VectorDBBench's leaderboard tooling looks for at the
    // top level of TestResult. Adding a new top-level field requires updating
    // both ends.
    let expected = BTreeSet::from([
        "vbench_schema_version".to_string(),
        "task_label".to_string(),
        "db_config".to_string(),
        "case_config".to_string(),
        "task_config".to_string(),
        "metrics".to_string(),
        "timestamps".to_string(),
    ]);

    assert_eq!(
        keys, expected,
        "top-level TestResult keys drifted from the expected schema",
    );
}

#[test]
fn schema_metrics_block_has_required_fields() {
    let result = synthetic_result();
    let json = serde_json::to_value(&result).unwrap();
    let metrics = json
        .get("metrics")
        .and_then(|v| v.as_object())
        .expect("metrics is an object");

    // The leaderboard reads these by exact name. Renaming any of them
    // makes our numbers silently uncomparable.
    let required = [
        "load_duration",
        "optimize_duration",
        "recall",
        "ndcg",
        "serial_latency_avg",
        "serial_latency_p50",
        "serial_latency_p95",
        "serial_latency_p99",
        "serial_query_count",
    ];
    for field in required {
        assert!(
            metrics.contains_key(field),
            "metrics is missing required field {field}",
        );
    }
}

#[test]
fn schema_case_config_has_required_fields() {
    let result = synthetic_result();
    let json = serde_json::to_value(&result).unwrap();
    let case = json
        .get("case_config")
        .and_then(|v| v.as_object())
        .expect("case_config is an object");

    let required = [
        "dataset",
        "dim",
        "metric",
        "recall_k",
        "num_train",
        "num_test",
    ];
    for field in required {
        assert!(
            case.contains_key(field),
            "case_config is missing required field {field}",
        );
    }
}

#[test]
fn schema_db_config_includes_db_version() {
    // Critical field for drift attribution: every published result must
    // identify which DB version produced it.
    let result = synthetic_result();
    let json = serde_json::to_value(&result).unwrap();
    assert!(
        json["db_config"]["db_version"].is_string(),
        "db_config.db_version must be present",
    );
}

#[test]
fn schema_round_trips_through_serde() {
    let original = synthetic_result();
    let json = serde_json::to_string(&original).unwrap();
    let decoded: TestResult = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.task_label, original.task_label);
    assert_eq!(decoded.metrics.recall, original.metrics.recall);
    assert_eq!(
        decoded.metrics.serial_latency_p99,
        original.metrics.serial_latency_p99
    );
}

#[test]
fn schema_timestamps_host_present() {
    let result = synthetic_result();
    let json = serde_json::to_value(&result).unwrap();
    let host = &json["timestamps"]["host"];
    assert!(host.is_object(), "timestamps.host must be an object");

    // host snapshot must surface the bare minimum so reviewers can recognise
    // the box.
    let mut all_keys = BTreeSet::new();
    collect_keys_recursive(host, "", &mut all_keys);
    for required in [
        "hostname",
        "os",
        "arch",
        "cpu_brand",
        "cpu_cores",
        "total_memory_bytes",
    ] {
        assert!(
            all_keys.contains(required),
            "host snapshot missing field: {required}",
        );
    }
}
