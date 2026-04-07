//! `vbench inspect <result.json>` — pretty-print a TestResult document.
//!
//! Renders the headline numbers (recall, ndcg, durations, latencies)
//! followed by the full JSON for cross-referencing against upstream
//! VectorDBBench's leaderboard tooling.

use std::path::Path;

use vbench_core::TestResult;

pub fn inspect(path: &Path) -> anyhow::Result<()> {
    let raw = std::fs::read_to_string(path)?;
    let result: TestResult = serde_json::from_str(&raw)?;

    println!("=== {} ===", path.display());
    println!("run_id     : {}", result.run_id);
    println!("task_label : {}", result.task_label);
    println!("timestamp  : {}", result.timestamp);
    println!("results    : {}", result.results.len());
    println!();

    for (i, case) in result.results.iter().enumerate() {
        println!("--- result[{i}] ---");
        let m = &case.metrics;
        let tc = &case.task_config;
        println!("  db              : {}", tc.db);
        println!("  case_id         : {}", tc.case_config.case_id);
        println!("  k               : {}", tc.case_config.k);
        println!("  stages          : [{}]", tc.stages.join(", "));
        println!("  label           : {}", case.label);
        println!();
        println!("  insert_duration : {:>10.2}  s", m.insert_duration);
        println!("  optimize_dur    : {:>10.2}  s", m.optimize_duration);
        println!(
            "  load_duration   : {:>10.2}  s  (insert + optimize)",
            m.load_duration
        );
        println!();
        println!("  recall          : {:>10.4}", m.recall);
        println!("  ndcg            : {:>10.4}", m.ndcg);
        println!();
        println!("  qps             : {:>10.2}  q/s", m.qps);
        println!(
            "  serial_lat_p99  : {:>10.4}  s  ({:.2} ms)",
            m.serial_latency_p99,
            m.serial_latency_p99 * 1000.0
        );
        println!(
            "  serial_lat_p95  : {:>10.4}  s  ({:.2} ms)",
            m.serial_latency_p95,
            m.serial_latency_p95 * 1000.0
        );
        println!();
    }

    println!("--- raw JSON ---");
    println!("{raw}");
    Ok(())
}
