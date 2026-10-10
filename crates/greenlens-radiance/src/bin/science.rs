fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let output = args
        .iter()
        .position(|s| s == "--output")
        .and_then(|i| args.get(i + 1))
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap_or_default())
                .join("GreenLens/verification/science")
        });
    greenlens_radiance::science::run_suite(&output, args.iter().any(|s| s == "--fixtures-only"))?;
    Ok(())
}
