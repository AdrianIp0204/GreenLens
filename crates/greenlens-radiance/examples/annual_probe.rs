//! Bounded developer evidence driver. The application CLI uses the shared
//! runner; this executable isolates adapter science and resource investigation.
use anyhow::{ensure, Context, Result};
use greenlens_radiance::{
    annual::{self, AnnualProfile},
    Engine,
};
use std::{fs, path::Path, sync::atomic::AtomicBool};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    ensure!(
        (5..=6).contains(&args.len()),
        "Usage: annual_probe MODEL_JSON NORMALIZED_EPW SCRATCH shipping|reference [INTERVAL]"
    );
    let model = greenlens_core::validate_model(&serde_json::from_slice(&fs::read(&args[1])?)?)?;
    let weather = fs::read(&args[2])?;
    let profile = match args[4].as_str() {
        "shipping" => AnnualProfile::shipping(),
        "reference" => AnnualProfile::reference(),
        _ => anyhow::bail!("Unknown complete numerical profile"),
    };
    let selected = args
        .get(5)
        .map(|value| value.parse::<usize>().context("Invalid interval"))
        .transpose()?;
    let engine = Engine::discover(None)?;
    let result = annual::simulate(
        &engine,
        &model,
        &weather,
        selected,
        &profile,
        Path::new(&args[3]),
        &AtomicBool::new(false),
        |line| eprintln!("{line}"),
    )?;
    let mut count = 0usize;
    let mut minimum = f64::INFINITY;
    let mut maximum = 0.0_f64;
    let mut sum = 0.0;
    result.visit_hours(|_, values| {
        for &rgb in values {
            let lux = greenlens_core::rgb_to_lux(rgb)?;
            minimum = minimum.min(lux);
            maximum = maximum.max(lux);
            sum += lux;
            count += 1;
        }
        Ok(())
    })?;
    let summary = serde_json::json!({"status":"exploratory native adapter execution; not release acceptance", "profile":result.profile, "engine":result.identity, "sensor_count":result.sensor_count, "interval_count":result.interval_indices.len(), "value_count":count, "minimum_lux":minimum, "mean_lux":sum/count as f64, "maximum_lux":maximum, "attempted_direct_rays":result.recipe.direct_batches.iter().map(|b|b.attempted_rays).sum::<usize>(), "emitted_direct_rays":result.recipe.direct_batches.iter().map(|b|b.emitted_rays).sum::<usize>(), "direct_batches":result.recipe.direct_batches.len(), "resources":result.resources});
    fs::write(
        Path::new(&args[3]).join("probe-summary.json"),
        serde_json::to_vec_pretty(&summary)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}
