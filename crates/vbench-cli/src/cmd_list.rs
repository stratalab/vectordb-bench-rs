//! `vbench list-datasets` and `vbench list-adapters`.

use vbench_core::CATALOG;

pub fn list_datasets() -> anyhow::Result<()> {
    println!(
        "{:<14}  {:<8}  {:<10}  {:<10}  {:<8}  description",
        "id", "dim", "metric", "train", "test"
    );
    println!("{}", "-".repeat(80));
    for spec in CATALOG {
        let metric = match spec.metric {
            vbench_core::Metric::Cosine => "cosine",
            vbench_core::Metric::L2 => "l2",
            vbench_core::Metric::Ip => "ip",
        };
        println!(
            "{:<14}  {:<8}  {:<10}  {:<10}  {:<8}  {}",
            spec.id, spec.dim, metric, spec.num_train, spec.num_test, spec.display_name,
        );
    }
    Ok(())
}

pub fn list_adapters() -> anyhow::Result<()> {
    println!("Adapters compiled into this build:");
    println!();
    let adapters = compiled_in_adapters();
    if adapters.is_empty() {
        println!("  (none — rebuild with `--features strata` or `--features all-adapters`)");
    } else {
        for a in adapters {
            println!("  - {a}");
        }
    }
    Ok(())
}

fn compiled_in_adapters() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = vec![];
    if cfg!(feature = "strata") {
        out.push("strata");
    }
    out
}
