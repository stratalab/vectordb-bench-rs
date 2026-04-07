//! `vbench cache show` and `vbench cache clear`.

use std::io::{self, Write};
use std::path::PathBuf;

use vbench_core::{cache_dir_for, default_cache_root, CATALOG};

pub fn show(cache_dir: Option<PathBuf>) -> anyhow::Result<()> {
    let root = cache_dir.unwrap_or_else(default_cache_root);
    println!("Cache root: {}", root.display());
    println!();

    if !root.exists() {
        println!("(cache root does not exist yet)");
        return Ok(());
    }

    let mut total_bytes: u64 = 0;
    println!("{:<24}  {:>12}  complete", "dataset", "size");
    println!("{}", "-".repeat(60));
    for spec in CATALOG {
        let dir = cache_dir_for(&root, spec.cache_subdir);
        if !dir.exists() {
            println!("{:<24}  {:>12}", spec.id, "(not cached)");
            continue;
        }
        let size = dir_size_bytes(&dir);
        total_bytes += size;
        let complete = dir.join(".complete").exists();
        println!(
            "{:<24}  {:>12}  {}",
            spec.id,
            human_bytes(size),
            if complete { "yes" } else { "no" },
        );
    }
    println!("{}", "-".repeat(60));
    println!("{:<24}  {:>12}", "total", human_bytes(total_bytes));
    Ok(())
}

pub fn clear(cache_dir: Option<PathBuf>, yes: bool) -> anyhow::Result<()> {
    let root = cache_dir.unwrap_or_else(default_cache_root);
    if !root.exists() {
        println!("Cache root does not exist; nothing to clear.");
        return Ok(());
    }

    let size = dir_size_bytes(&root);
    println!(
        "About to remove cache root {} ({}).",
        root.display(),
        human_bytes(size)
    );

    if !yes {
        print!("Continue? [y/N] ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let trimmed = input.trim().to_lowercase();
        if trimmed != "y" && trimmed != "yes" {
            println!("Aborted.");
            return Ok(());
        }
    }

    std::fs::remove_dir_all(&root)?;
    println!("Removed {}.", root.display());
    Ok(())
}

fn dir_size_bytes(path: &std::path::Path) -> u64 {
    let mut total: u64 = 0;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if let Ok(meta) = entry.metadata() {
                if meta.is_file() {
                    total += meta.len();
                } else if meta.is_dir() {
                    total += dir_size_bytes(&p);
                }
            }
        }
    }
    total
}

fn human_bytes(bytes: u64) -> String {
    const UNITS: &[(&str, u64)] = &[
        ("GiB", 1024 * 1024 * 1024),
        ("MiB", 1024 * 1024),
        ("KiB", 1024),
    ];
    for (label, scale) in UNITS {
        if bytes >= *scale {
            return format!("{:.1} {label}", bytes as f64 / *scale as f64);
        }
    }
    format!("{bytes} B")
}
