use std::fs;
use std::process::Command;

#[test]
fn cli_doctor_and_run_flow() {
    let bin = env!("CARGO_BIN_EXE_axon-cli");
    let tmp = std::env::temp_dir().join("axon_cli_test_ws");
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).unwrap();
    fs::write(tmp.join("README.md"), "# Test Project\nHello Axon").unwrap();

    // 1. Run doctor
    let doc = Command::new(bin)
        .arg("doctor")
        .current_dir(&tmp)
        .output()
        .expect("failed to execute doctor");
    assert!(doc.status.success());
    let stdout = String::from_utf8_lossy(&doc.stdout);
    assert!(stdout.contains("Axon Runtime Doctor"));

    // 2. Run offline agent loop
    let run = Command::new(bin)
        .args(["run", "Inspect README and summarize", "--json"])
        .current_dir(&tmp)
        .output()
        .expect("failed to execute run");
    assert!(run.status.success());
    let run_out = String::from_utf8_lossy(&run.stdout);
    assert!(run_out.contains("\"status\": \"completed\""));
    assert!(run_out.contains("Hello Axon"));

    let _ = fs::remove_dir_all(&tmp);
}
