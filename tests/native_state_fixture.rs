#![cfg(feature = "native-state-fixture")]

use serde_json::{json, Value};
use std::process::{Command, Output};

fn run(db: &str, profile: &str, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_apollo"))
        .args(["native-state-fixture", "--db", db, "--profile", profile])
        .args(args)
        .output()
        .unwrap()
}

fn json_output(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn versions_survive_restart_and_reject_stale_or_foreign_edits() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("state.sqlite");
    let db = db.to_str().unwrap();
    assert_eq!(json_output(run(db, "alice", &["read"])), Value::Null);
    let first = json_output(run(
        db,
        "alice",
        &[
            "edit",
            "--slot",
            "0",
            "--value",
            "12",
            "--source",
            "synthetic-1",
        ],
    ));
    let version = first["version"].to_string();
    let second = json_output(run(
        db,
        "alice",
        &[
            "edit",
            "--slot",
            "1",
            "--value",
            "-9",
            "--source",
            "synthetic-2",
            "--expected",
            &version,
        ],
    ));
    assert_eq!(second["values"], json!([12, -9]));
    assert_eq!(second["version"]["generation"], 2);
    assert_eq!(second["source"], "synthetic-2");
    let stale = run(
        db,
        "alice",
        &[
            "edit",
            "--slot",
            "0",
            "--value",
            "99",
            "--source",
            "stale",
            "--expected",
            &version,
        ],
    );
    assert!(!stale.status.success());
    assert!(String::from_utf8_lossy(&stale.stderr).contains("conflict"));
    let foreign = run(
        db,
        "bob",
        &[
            "edit",
            "--slot",
            "0",
            "--value",
            "99",
            "--source",
            "foreign",
            "--expected",
            &second["version"].to_string(),
        ],
    );
    assert!(!foreign.status.success());
    let bob = json_output(run(
        db,
        "bob",
        &[
            "edit",
            "--slot",
            "1",
            "--value",
            "7",
            "--source",
            "synthetic-b",
        ],
    ));
    assert_eq!(bob["values"], json!([0, 7]));
    assert_eq!(json_output(run(db, "alice", &["read"])), second);
    assert_eq!(json_output(run(db, "bob", &["read"])), bob);
    let conn = rusqlite::Connection::open(db).unwrap();
    let mut incompatible = second["version"].clone();
    incompatible["model"] = json!("different-model");
    let mismatch = run(
        db,
        "alice",
        &[
            "edit",
            "--slot",
            "0",
            "--value",
            "42",
            "--source",
            "wrong-model",
            "--expected",
            &incompatible.to_string(),
        ],
    );
    assert!(!mismatch.status.success());
    assert!(String::from_utf8_lossy(&mismatch.stderr).contains("conflict"));
    assert_eq!(json_output(run(db, "alice", &["read"])), second);
    conn.execute(
        "UPDATE snapshots SET schema='wrong-schema' WHERE profile='alice' AND generation=2",
        [],
    )
    .unwrap();
    let wrong_schema = run(db, "alice", &["read"]);
    assert!(!wrong_schema.status.success());
    assert!(String::from_utf8_lossy(&wrong_schema.stderr).contains("identity or integrity"));
    conn.execute("UPDATE snapshots SET schema='synthetic-two-values-v1' WHERE profile='alice' AND generation=2", []).unwrap();
    conn.execute(
        "UPDATE snapshots SET state=x'5b39392c39395d' WHERE profile='alice' AND generation=2",
        [],
    )
    .unwrap();
    let corrupt = run(db, "alice", &["read"]);
    assert!(!corrupt.status.success());
    assert!(String::from_utf8_lossy(&corrupt.stderr).contains("integrity"));
    assert_eq!(json_output(run(db, "bob", &["read"])), bob);
}

#[test]
fn concurrent_writers_cannot_both_publish_from_one_version() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("state.sqlite");
    let db = db.to_str().unwrap();
    let initial = json_output(run(
        db,
        "p",
        &["edit", "--slot", "0", "--value", "1", "--source", "seed"],
    ));
    let version = initial["version"].to_string();
    let outputs = std::thread::scope(|scope| {
        let a = scope.spawn(|| {
            run(
                db,
                "p",
                &[
                    "edit",
                    "--slot",
                    "0",
                    "--value",
                    "2",
                    "--source",
                    "a",
                    "--expected",
                    &version,
                ],
            )
        });
        let b = scope.spawn(|| {
            run(
                db,
                "p",
                &[
                    "edit",
                    "--slot",
                    "0",
                    "--value",
                    "3",
                    "--source",
                    "b",
                    "--expected",
                    &version,
                ],
            )
        });
        [a.join().unwrap(), b.join().unwrap()]
    });
    assert_eq!(outputs.iter().filter(|o| o.status.success()).count(), 1);
    let failure = outputs.iter().find(|o| !o.status.success()).unwrap();
    assert!(String::from_utf8_lossy(&failure.stderr).contains("conflict"));
    let current = json_output(run(db, "p", &["read"]));
    assert_eq!(current["version"]["generation"], 2);
    assert!([json!([2, 0]), json!([3, 0])].contains(&current["values"]));
}
