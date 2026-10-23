//! Fixed-fixture cold, unchanged and incremental parse observations.

use std::{hint::black_box, time::Instant};
use tinytools_code::{CodeEngine, Language};

fn main() -> Result<(), tinytools_code::CodeError> {
    let source = include_str!("../tests/fixtures/analysis.rs");
    let iterations = 100;
    let start = Instant::now();
    for _ in 0..iterations {
        black_box(CodeEngine::default().analyze("fixture", Language::Rust, source)?);
    }
    let cold = start.elapsed();
    let mut engine = CodeEngine::default();
    let report = engine.analyze("fixture", Language::Rust, source)?;
    let start = Instant::now();
    for _ in 0..iterations {
        black_box(engine.analyze("fixture", Language::Rust, source)?);
    }
    let cached = start.elapsed();
    let changed = source.replace("value * 2", "value * 3");
    let start = Instant::now();
    for index in 0..iterations {
        black_box(engine.analyze(
            "fixture",
            Language::Rust,
            if index % 2 == 0 { &changed } else { source },
        )?);
    }
    println!(
        "iterations={iterations} source_bytes={} folded_bytes={} symbols={} cold_us={} cached_us={} incremental_us={}",
        source.len(),
        report.skeleton.len(),
        report.symbols.len(),
        cold.as_micros(),
        cached.as_micros(),
        start.elapsed().as_micros()
    );
    Ok(())
}
