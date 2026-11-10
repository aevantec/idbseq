//! Group commit for strict collections (US-0039).
//!
//! One background thread owns ID-issuing writes. Callers queue a job and wait.
//! The thread takes the first job, adds every job already queued (up to
//! `max_batch`, optionally waiting up to `max_wait` for more), applies them all
//! in one write transaction and commits once. Only after that commit is durable
//! does any caller get its IDs. If the commit fails, every job in the group fails.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::Instant;

use ::redb::{Database, ReadableTable, Table};

use super::codec::{backend, decode, encode};
use super::{COLLECTIONS, GroupCommitConfig, IDEMPOTENCY};
use crate::collection::{Collection, Durability};
use crate::store::idempotency::{self, Record};
use crate::store::{AdvanceRequest, Issued, StoreError};

/// What the committer did with one request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Outcome {
    Issued(Issued),
    /// The collection uses block durability; the caller must use the block path.
    BlockMode,
}

type Reply = Result<Outcome, StoreError>;

struct Job {
    request: AdvanceRequest,
    reply: SyncSender<Reply>,
}

pub(super) struct Committer {
    sender: Option<Sender<Job>>,
    worker: Option<JoinHandle<()>>,
    #[cfg_attr(not(test), allow(dead_code))] // set only by tests
    fail_next_commit: Arc<AtomicBool>,
    commits: Arc<AtomicU64>,
}

impl Committer {
    pub fn start(db: Arc<Database>, config: GroupCommitConfig) -> Self {
        let (sender, receiver) = mpsc::channel();
        let fail_next_commit = Arc::new(AtomicBool::new(false));
        let commits = Arc::new(AtomicU64::new(0));
        let probes = Probes {
            fail: Arc::clone(&fail_next_commit),
            commits: Arc::clone(&commits),
        };
        let worker = thread::Builder::new()
            .name("idbseq-committer".into())
            .spawn(move || run(&db, &receiver, config, &probes))
            .ok();
        Self {
            sender: Some(sender),
            worker,
            fail_next_commit,
            commits,
        }
    }

    /// Queue a request and wait until its group is committed.
    pub fn submit(&self, request: AdvanceRequest) -> Reply {
        let stopped = || StoreError::Backend("group committer is not running".into());
        let (reply, response) = mpsc::sync_channel(1);
        let sender = self.sender.as_ref().ok_or_else(stopped)?;
        sender.send(Job { request, reply }).map_err(|_| stopped())?;
        response.recv().map_err(|_| stopped())?
    }

    #[cfg(test)]
    pub fn fail_next_commit(&self) {
        self.fail_next_commit.store(true, Ordering::SeqCst);
    }

    /// Group transactions committed so far.
    pub fn commits(&self) -> u64 {
        self.commits.load(Ordering::Relaxed)
    }
}

/// Hooks observed by tests: inject one commit failure, count commits.
struct Probes {
    fail: Arc<AtomicBool>,
    commits: Arc<AtomicU64>,
}

impl Drop for Committer {
    fn drop(&mut self) {
        // Closing the channel ends the worker loop once queued jobs are done.
        drop(self.sender.take());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn run(db: &Database, receiver: &Receiver<Job>, config: GroupCommitConfig, probes: &Probes) {
    while let Ok(first) = receiver.recv() {
        let jobs = collect_group(first, receiver, config);
        let replies = commit_group(db, &jobs, probes).unwrap_or_else(|e| vec![Err(e); jobs.len()]);
        for (job, reply) in jobs.into_iter().zip(replies) {
            // A caller that gave up waiting is not an error for the others.
            let _ = job.reply.send(reply);
        }
    }
}

fn collect_group(first: Job, receiver: &Receiver<Job>, config: GroupCommitConfig) -> Vec<Job> {
    let deadline = Instant::now() + config.max_wait;
    let mut jobs = vec![first];
    while jobs.len() < config.max_batch.max(1) {
        let next = match receiver.try_recv() {
            Ok(job) => Some(job),
            Err(TryRecvError::Disconnected) => None,
            Err(TryRecvError::Empty) => {
                let left = deadline.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    None
                } else {
                    receiver.recv_timeout(left).ok()
                }
            }
        };
        match next {
            Some(job) => jobs.push(job),
            None => break,
        }
    }
    jobs
}

/// Apply every job in one transaction. `Err` means the whole group failed.
fn commit_group(db: &Database, jobs: &[Job], probes: &Probes) -> Result<Vec<Reply>, StoreError> {
    let txn = db.begin_write().map_err(backend)?;
    let (replies, wrote) = {
        let mut collections = txn.open_table(COLLECTIONS).map_err(backend)?;
        let mut keys = txn.open_table(IDEMPOTENCY).map_err(backend)?;
        let mut wrote = false;
        let mut replies = Vec::with_capacity(jobs.len());
        for job in jobs {
            let (reply, changed) = apply(&mut collections, &mut keys, &job.request)?;
            wrote |= changed;
            replies.push(reply);
        }
        (replies, wrote)
    };
    if probes.fail.swap(false, Ordering::SeqCst) {
        return Err(StoreError::Backend("injected commit failure".into()));
    }
    if wrote {
        txn.commit().map_err(backend)?;
        probes.commits.fetch_add(1, Ordering::Relaxed);
    }
    Ok(replies)
}

/// Apply one request inside the group's transaction. Returns its reply and whether
/// it wrote anything. `Err` is a storage failure that aborts the whole group.
fn apply(
    collections: &mut Table<&str, &[u8]>,
    keys: &mut Table<&str, &[u8]>,
    request: &AdvanceRequest,
) -> Result<(Reply, bool), StoreError> {
    if let Some(key) = &request.idempotency {
        let existing: Option<Record> = keys
            .get(key.key.as_str())
            .map_err(backend)?
            .map(|guard| decode(guard.value()))
            .transpose()?;
        match idempotency::replay(existing.as_ref(), key) {
            Ok(Some(range)) => {
                return Ok((
                    Ok(Outcome::Issued(Issued {
                        range,
                        replayed: true,
                    })),
                    false,
                ));
            }
            Ok(None) => {}
            Err(mismatch) => return Ok((Err(mismatch), false)),
        }
    }
    let name = request.name.as_str();
    let current: Option<Collection> = collections
        .get(name)
        .map_err(backend)?
        .map(|guard| decode(guard.value()))
        .transpose()?;
    let Some(current) = current else {
        return Ok((Err(StoreError::NotFound(name.to_owned())), false));
    };
    if matches!(current.spec().durability, Durability::Block { .. }) {
        return Ok((Ok(Outcome::BlockMode), false));
    }
    let Ok((updated, range)) = current.advance(request.count) else {
        return Ok((Err(StoreError::Exhausted(name.to_owned())), false));
    };
    collections
        .insert(name, encode(&updated)?.as_slice())
        .map_err(backend)?;
    if let Some(key) = &request.idempotency {
        keys.insert(
            key.key.as_str(),
            encode(&Record::new(key, range))?.as_slice(),
        )
        .map_err(backend)?;
    }
    Ok((
        Ok(Outcome::Issued(Issued {
            range,
            replayed: false,
        })),
        true,
    ))
}
