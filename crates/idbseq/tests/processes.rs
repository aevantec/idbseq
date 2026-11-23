//! Several processes on one file in shared mode never get the same ID.
//!
//! The test runs copies of this test binary as workers: each runs only
//! `worker`, which does nothing unless the parent set `IDBSEQ_WORKER_FILE`.
#![allow(clippy::expect_used)] // test code: a failed setup step should panic

use std::process::{Command, Stdio};
use std::time::Duration;

use idbseq::{CollectionSpec, Database, Durability, Options};

const FILE_VAR: &str = "IDBSEQ_WORKER_FILE";
const WORKERS: usize = 4;
const IDS_PER_WORKER: usize = 50;

fn shared() -> Options {
    Options::shared().busy_timeout(Duration::from_secs(60))
}

#[test]
#[ignore = "run by processes_sharing_a_file_never_get_the_same_id"]
fn worker() {
    let Ok(path) = std::env::var(FILE_VAR) else {
        return;
    };
    let ids = Database::open_with(path, shared()).expect("open shared");
    for _ in 0..IDS_PER_WORKER {
        println!("strict {}", ids.next_id("strict").expect("strict"));
        println!("block {}", ids.next_id("block").expect("block"));
    }
}

#[test]
fn processes_sharing_a_file_never_get_the_same_id() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("shared.idbseq");
    let setup = Database::open_with(&path, shared()).expect("open");
    setup
        .create_collection(CollectionSpec::named("strict"))
        .expect("create");
    setup
        .create_collection(CollectionSpec {
            durability: Durability::Block {
                size: 50.try_into().expect("non-zero"),
            },
            ..CollectionSpec::named("block")
        })
        .expect("create");

    let exe = std::env::current_exe().expect("test binary");
    // Start every worker before waiting for any, so they contend for the file.
    let children = (0..WORKERS)
        .map(|_| {
            Command::new(&exe)
                .args([
                    "worker",
                    "--exact",
                    "--ignored",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env(FILE_VAR, &path)
                .stdout(Stdio::piped())
                .spawn()
                .expect("start worker")
        })
        .collect::<Vec<_>>();
    let output = children
        .into_iter()
        .map(|child| {
            let done = child.wait_with_output().expect("worker output");
            assert!(done.status.success(), "a worker failed");
            String::from_utf8(done.stdout).expect("utf-8")
        })
        .collect::<String>();

    for collection in ["strict", "block"] {
        let mut ids = output
            .lines()
            // libtest may print its "test worker ..." label on the same line.
            .filter_map(|line| {
                let (_, id) = line.split_once(&format!("{collection} "))?;
                id.trim().parse::<i64>().ok()
            })
            .collect::<Vec<_>>();
        ids.sort_unstable();
        let issued = ids.len();
        ids.dedup();
        assert_eq!(
            issued,
            WORKERS * IDS_PER_WORKER,
            "{collection}: every worker finished"
        );
        assert_eq!(ids.len(), issued, "{collection}: an ID was issued twice");
        // Shared mode writes block collections like strict ones: no gaps.
        assert_eq!(
            ids.last().copied(),
            i64::try_from(issued).ok(),
            "{collection}: no gaps"
        );
    }
}
