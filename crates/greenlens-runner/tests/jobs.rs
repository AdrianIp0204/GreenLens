//! Application lifecycle checks. The cancellation test requires native Radiance;
//! absence fails explicitly and is never counted as a skipped acceptance check.
use greenlens_core::Model;
use greenlens_radiance::Profile;
use greenlens_runner::{Application, JobState};
use std::{
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

#[test]
fn missing_engine_is_failed_without_a_result_or_completed_project() {
    let app = Application::default();
    app.start(
        Model::default(),
        "A".into(),
        Some(PathBuf::from("missing-greenlens-engine-for-test")),
        Profile::shipping(),
    )
    .unwrap();
    let status = app.wait().unwrap();
    assert_eq!(status.state, JobState::Failed);
    assert!(status.result.is_none());
    assert!(status.error.unwrap().contains("unavailable"));
    let target = tempfile::tempdir().unwrap();
    assert!(app
        .collect(target.path(), &greenlens_runner::example(), None)
        .is_err());
    assert!(!target.path().join("project.json").exists());
}

#[test]
fn native_cancellation_terminates_run_and_never_promotes_partial_results() {
    greenlens_radiance::Engine::discover(None)
        .expect("Required native engine unavailable; cancellation acceptance cannot be skipped");
    let app = Application::default();
    app.start(Model::default(), "A".into(), None, Profile::shipping())
        .unwrap();
    assert!(app
        .start(Model::default(), "B".into(), None, Profile::shipping())
        .is_err());
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let status = app.status().unwrap();
        if status
            .log
            .iter()
            .any(|l| l.contains("Running native Radiance"))
        {
            break;
        }
        assert!(
            !matches!(
                status.state,
                JobState::Failed | JobState::Completed | JobState::Cancelled
            ),
            "unexpected early terminal state: {status:?}"
        );
        assert!(
            Instant::now() < deadline,
            "Native rtrace did not begin within 30 seconds"
        );
        thread::sleep(Duration::from_millis(20));
    }
    // Let rtrace actually spawn, then request cancellation through the same
    // application service used by the native desktop.
    thread::sleep(Duration::from_millis(100));
    app.cancel().unwrap();
    let status = app.wait().unwrap();
    assert_eq!(status.state, JobState::Cancelled, "{status:?}");
    assert!(status.result.is_none());
    let target = tempfile::tempdir().unwrap();
    assert!(app
        .collect(target.path(), &greenlens_runner::example(), None)
        .is_err());
    assert!(!target.path().join("project.json").exists());
}
