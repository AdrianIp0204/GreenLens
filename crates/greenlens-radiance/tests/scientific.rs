#[test]
fn required_native_engine_scientific_suite() {
    let output = std::path::PathBuf::from(
        std::env::var_os("LOCALAPPDATA")
            .expect("Native Windows LOCALAPPDATA is required for release acceptance"),
    )
    .join("GreenLens/verification/required-scientific-suite");
    greenlens_radiance::science::run_suite(&output, false)
        .expect("Required engine/scientific acceptance failed; never treat this as a skipped test");
}
