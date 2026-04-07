//! Strict schema-fidelity tests for the `TestResult` JSON we emit.
//!
//! Validates field names against VectorDBBench's
//! `vectordb_bench/models.py:TestResult`, `CaseResult`, `TaskConfig`,
//! `CaseConfig`, and `vectordb_bench/metric.py:Metric`. Cross-referenced
//! against a real published file (ElasticCloud
//! `result_20260403_standard_elasticcloud.json`).
//!
//! These tests are the long-term guard against schema drift. If anyone
//! renames a field on either side they fail loudly, instead of producing
//! silently-incomparable JSON.

use std::collections::BTreeSet;

use vbench_core::{
    result_label, CaseConfig, CaseResult, ConcurrencySearchConfig, ResultMetric, TaskConfig,
    TestResult,
};

fn synthetic_result() -> TestResult {
    let metrics = ResultMetric {
        max_load_count: 0,
        insert_duration: 1.0,
        optimize_duration: 0.5,
        load_duration: 1.5,
        qps: 0.0,
        serial_latency_p99: 0.0106, // seconds — 10.6 ms
        serial_latency_p95: 0.0073,
        recall: 0.95,
        ndcg: 0.93,
        ..ResultMetric::default()
    };

    let case = CaseResult {
        metrics,
        task_config: TaskConfig {
            db: "TestDB".to_string(),
            db_config: serde_json::json!({
                "db_label": "TestDB",
                "version": "1.0.0",
                "note": "synthetic test",
            }),
            db_case_config: serde_json::json!({
                "metric_type": "COSINE",
            }),
            case_config: CaseConfig {
                case_id: 5, // Performance768D1M
                custom_case: None,
                k: 100,
                concurrency_search_config: ConcurrencySearchConfig::default(),
            },
            stages: vec![
                "drop_old".to_string(),
                "load".to_string(),
                "search_serial".to_string(),
            ],
            load_concurrency: 1,
        },
        label: result_label::NORMAL.to_string(),
    };

    TestResult {
        run_id: TestResult::new_run_id(),
        task_label: "synthetic-task".to_string(),
        results: vec![case],
        file_fmt: "result_{}_{}_{}.json".to_string(),
        timestamp: 1_700_000_000.0,
    }
}

#[test]
fn schema_top_level_keys_match_upstream() {
    let result = synthetic_result();
    let json = serde_json::to_value(&result).unwrap();
    let obj = json.as_object().expect("top-level is an object");
    let keys: BTreeSet<String> = obj.keys().cloned().collect();

    // From `vectordb_bench/models.py:TestResult`.
    let expected = BTreeSet::from([
        "run_id".to_string(),
        "task_label".to_string(),
        "results".to_string(),
        "file_fmt".to_string(),
        "timestamp".to_string(),
    ]);

    assert_eq!(
        keys, expected,
        "top-level TestResult keys drifted from upstream",
    );
}

#[test]
fn schema_case_result_keys_match_upstream() {
    let result = synthetic_result();
    let json = serde_json::to_value(&result).unwrap();
    let case = json["results"][0]
        .as_object()
        .expect("results[0] is an object");
    let keys: BTreeSet<String> = case.keys().cloned().collect();

    // From `vectordb_bench/models.py:CaseResult`.
    let expected = BTreeSet::from([
        "metrics".to_string(),
        "task_config".to_string(),
        "label".to_string(),
    ]);

    assert_eq!(keys, expected, "CaseResult keys drifted from upstream");
}

#[test]
fn schema_metric_block_has_every_upstream_field() {
    let result = synthetic_result();
    let json = serde_json::to_value(&result).unwrap();
    let metrics = json["results"][0]["metrics"]
        .as_object()
        .expect("metrics is an object");

    // Every field from `vectordb_bench/metric.py:Metric`.
    // Cross-checked against the published ElasticCloud result file.
    let required = [
        // load
        "max_load_count",
        // performance & streaming
        "insert_duration",
        "optimize_duration",
        "load_duration",
        // performance
        "qps",
        "serial_latency_p99",
        "serial_latency_p95",
        "recall",
        "ndcg",
        // concurrent
        "conc_num_list",
        "conc_qps_list",
        "conc_latency_p99_list",
        "conc_latency_p95_list",
        "conc_latency_avg_list",
        // streaming
        "st_ideal_insert_duration",
        "st_search_stage_list",
        "st_search_time_list",
        "st_max_qps_list_list",
        "st_recall_list",
        "st_ndcg_list",
        "st_serial_latency_p99_list",
        "st_serial_latency_p95_list",
        "st_conc_failed_rate_list",
        "st_conc_num_list_list",
        "st_conc_qps_list_list",
        "st_conc_latency_p99_list_list",
        "st_conc_latency_p95_list_list",
        "st_conc_latency_avg_list_list",
    ];
    for field in required {
        assert!(
            metrics.contains_key(field),
            "metrics is missing required upstream field: {field}"
        );
    }
}

#[test]
fn schema_task_config_keys_match_upstream() {
    let result = synthetic_result();
    let json = serde_json::to_value(&result).unwrap();
    let tc = json["results"][0]["task_config"]
        .as_object()
        .expect("task_config is an object");
    let keys: BTreeSet<String> = tc.keys().cloned().collect();

    // From `vectordb_bench/models.py:TaskConfig`.
    let expected = BTreeSet::from([
        "db".to_string(),
        "db_config".to_string(),
        "db_case_config".to_string(),
        "case_config".to_string(),
        "stages".to_string(),
        "load_concurrency".to_string(),
    ]);

    assert_eq!(keys, expected, "TaskConfig keys drifted from upstream");
}

#[test]
fn schema_case_config_keys_match_upstream() {
    let result = synthetic_result();
    let json = serde_json::to_value(&result).unwrap();
    let cc = json["results"][0]["task_config"]["case_config"]
        .as_object()
        .expect("case_config is an object");
    let keys: BTreeSet<String> = cc.keys().cloned().collect();

    // From `vectordb_bench/models.py:CaseConfig`.
    let expected = BTreeSet::from([
        "case_id".to_string(),
        "custom_case".to_string(),
        "k".to_string(),
        "concurrency_search_config".to_string(),
    ]);

    assert_eq!(keys, expected, "CaseConfig keys drifted from upstream");
}

#[test]
fn schema_concurrency_search_config_keys_match_upstream() {
    let result = synthetic_result();
    let json = serde_json::to_value(&result).unwrap();
    let csc = json["results"][0]["task_config"]["case_config"]["concurrency_search_config"]
        .as_object()
        .expect("concurrency_search_config is an object");
    let keys: BTreeSet<String> = csc.keys().cloned().collect();

    let expected = BTreeSet::from([
        "num_concurrency".to_string(),
        "concurrency_duration".to_string(),
        "concurrency_timeout".to_string(),
    ]);

    assert_eq!(
        keys, expected,
        "ConcurrencySearchConfig keys drifted from upstream"
    );
}

#[test]
fn schema_round_trips_through_serde() {
    let original = synthetic_result();
    let json = serde_json::to_string(&original).unwrap();
    let decoded: TestResult = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.task_label, original.task_label);
    assert_eq!(decoded.results.len(), 1);
    assert_eq!(decoded.results[0].label, ":)");
    assert_eq!(decoded.results[0].task_config.case_config.case_id, 5);
}

#[test]
fn schema_latency_units_are_seconds_not_ms() {
    // Document the unit by reading back what we wrote and asserting the
    // value range. 0.0106 seconds = 10.6 ms; if anyone "fixes" this to
    // milliseconds the value would jump to 10.6.
    let result = synthetic_result();
    let json = serde_json::to_value(&result).unwrap();
    let p99 = json["results"][0]["metrics"]["serial_latency_p99"]
        .as_f64()
        .expect("serial_latency_p99 is a number");
    assert!(
        (p99 - 0.0106).abs() < 1e-9,
        "serial_latency_p99 is being converted; should remain in seconds (0.0106), got {p99}",
    );
}

#[test]
fn schema_load_duration_equals_insert_plus_optimize() {
    // Upstream's invariant: load_duration = insert_duration + optimize_duration.
    let result = synthetic_result();
    let m = &result.results[0].metrics;
    assert!(
        (m.load_duration - (m.insert_duration + m.optimize_duration)).abs() < 1e-9,
        "load_duration ({}) != insert_duration ({}) + optimize_duration ({})",
        m.load_duration,
        m.insert_duration,
        m.optimize_duration,
    );
}

#[test]
fn schema_run_id_is_uuid_hex_no_dashes() {
    // Upstream uses UUID4 hex form: 32 hex chars, no dashes.
    let id = TestResult::new_run_id();
    assert_eq!(id.len(), 32, "run_id should be 32 hex chars: {id}");
    assert!(
        id.chars().all(|c| c.is_ascii_hexdigit()),
        "run_id should be hex: {id}"
    );
    assert!(!id.contains('-'), "run_id should not contain dashes: {id}");
}
