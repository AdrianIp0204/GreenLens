//! Build-time Git identity shared by the runner and the isolated acceptance probe.
//! No runtime Git dependency, source mutation or private paths in the executable.

use std::{env, path::Path, process::Command};

fn git(directory: &Path, arguments: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("--no-optional-locks")
        .arg("-C")
        .arg(directory)
        .args(arguments)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_COMMON_DIR")
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

pub fn emit() {
    // A nonexistent input intentionally re-runs this small script every Cargo
    // invocation. Tracking HEAD/index alone misses new untracked files; watching
    // the whole repository recursively would scan node_modules and build caches.
    let out = env::var_os("OUT_DIR").expect("Cargo supplies OUT_DIR");
    let sentinel = Path::new(&out).join("greenlens-provenance-always-refresh");
    assert!(
        !sentinel.exists(),
        "provenance refresh sentinel must not exist"
    );
    println!("cargo:rerun-if-changed={}", sentinel.display());
    for name in [
        "GREENLENS_EXPECT_SOURCE_COMMIT",
        "GREENLENS_EXPECT_SOURCE_DIRTY",
        "GREENLENS_BUILD_SOURCE_COMMIT",
        "GREENLENS_BUILD_SOURCE_DIRTY",
    ] {
        println!("cargo:rerun-if-env-changed={name}");
    }

    let manifest = env::var_os("CARGO_MANIFEST_DIR").expect("Cargo supplies manifest directory");
    let directory = Path::new(&manifest);
    let commit = git(directory, &["rev-parse", "--verify", "HEAD"]);
    let dirty = git(
        directory,
        &[
            "-c",
            "core.fsmonitor=false",
            "status",
            "--porcelain=v1",
            "--untracked-files=normal",
            "--ignore-submodules=none",
        ],
    )
    .map(|status| !status.is_empty());
    let expected_commit = env::var("GREENLENS_EXPECT_SOURCE_COMMIT").ok();
    let expected_dirty = env::var("GREENLENS_EXPECT_SOURCE_DIRTY").ok();
    match (expected_commit, expected_dirty) {
        (None, None) => {}
        (Some(expected_commit), Some(expected_dirty)) => {
            assert!(expected_commit.len() == 40 && expected_commit.bytes().all(|b| b.is_ascii_hexdigit()), "expected source commit must be a full 40-character Git SHA");
            assert!(matches!(expected_dirty.as_str(), "true" | "false"), "expected dirty state must be true or false");
            assert_eq!(commit.as_deref(), Some(expected_commit.to_lowercase().as_str()), "explicit CI source commit differs from the checked-out Git revision");
            assert_eq!(dirty, Some(expected_dirty == "true"), "explicit CI dirty state differs from the checked-out working tree");
        }
        _ => panic!("source identity assertions require both GREENLENS_EXPECT_SOURCE_COMMIT and GREENLENS_EXPECT_SOURCE_DIRTY"),
    }

    if let (Some(commit), Some(dirty)) = (commit, dirty) {
        println!("cargo:rustc-env=GREENLENS_BUILD_SOURCE_COMMIT={commit}");
        println!("cargo:rustc-env=GREENLENS_BUILD_SOURCE_DIRTY={dirty}");
    } else {
        // With no rustc-env output, rustc would inherit these names from the
        // caller. Reject that unverified fallback instead of stamping an archive
        // with a caller-provided commit or falsely clean state.
        assert!(
            env::var_os("GREENLENS_BUILD_SOURCE_COMMIT").is_none()
                && env::var_os("GREENLENS_BUILD_SOURCE_DIRTY").is_none(),
            "unverifiable inherited build source identity: clear GREENLENS_BUILD_SOURCE_COMMIT and GREENLENS_BUILD_SOURCE_DIRTY; checked Git builds may use the GREENLENS_EXPECT_SOURCE_* assertion pair"
        );
        println!(
            "cargo:warning=Git identity unavailable; source provenance will be unknown, not clean"
        );
    }
}
