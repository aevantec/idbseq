//! US-0010: property tests. Random sequences of create / next / reset / delete /
//! update must behave identically on the redb store and the in-memory store, and
//! no ID may be issued twice unless the sequence was explicitly rewound.
#![allow(clippy::expect_used)] // test code: a failed setup step should panic

use std::collections::{HashMap, HashSet};
use std::num::{NonZeroU32, NonZeroU64};
use std::sync::Arc;
use std::time::SystemTime;

use idbseq_core::collection::{CollectionPatch, CollectionSpec, Durability, Id};
use idbseq_core::engine::{Engine, Limits, NextRequest};
use idbseq_core::store::{GroupCommitConfig, InMemoryStore, RedbStore};
use proptest::prelude::*;

const NAMES: [&str; 3] = ["a", "b", "c"];

#[derive(Debug, Clone)]
enum Op {
    Create {
        name: usize,
        start: Id,
        step: u64,
        max_offset: Id,
        block: Option<u32>,
    },
    Next {
        name: usize,
        count: Option<u32>,
        key: Option<u8>,
    },
    Reset {
        name: usize,
        value_offset: Option<Id>,
        allow_rewind: bool,
    },
    Delete {
        name: usize,
    },
    SetMax {
        name: usize,
        max_offset: Id,
    },
    SetBlock {
        name: usize,
        block: Option<u32>,
    },
}

fn op() -> impl Strategy<Value = Op> {
    let name = 0..NAMES.len();
    prop_oneof![
        (
            name.clone(),
            -50i64..50,
            1u64..5,
            0i64..200,
            proptest::option::of(1u32..20)
        )
            .prop_map(|(name, start, step, max_offset, block)| Op::Create {
                name,
                start,
                step,
                max_offset,
                block
            }),
        (
            name.clone(),
            proptest::option::of(1u32..30),
            proptest::option::of(0u8..3)
        )
            .prop_map(|(name, count, key)| Op::Next { name, count, key }),
        (name.clone(), proptest::option::of(0i64..100), any::<bool>()).prop_map(
            |(name, value_offset, allow_rewind)| Op::Reset {
                name,
                value_offset,
                allow_rewind
            }
        ),
        name.clone().prop_map(|name| Op::Delete { name }),
        (name.clone(), 0i64..300).prop_map(|(name, max_offset)| Op::SetMax { name, max_offset }),
        (name, proptest::option::of(1u32..20))
            .prop_map(|(name, block)| Op::SetBlock { name, block }),
    ]
}

fn durability(block: Option<u32>) -> Durability {
    block
        .and_then(NonZeroU32::new)
        .map_or(Durability::Strict, |size| Durability::Block { size })
}

/// Apply one op; return a comparable rendering of the outcome and any issued IDs.
fn apply(engine: &Engine, op: &Op, now: SystemTime) -> (String, Vec<Id>) {
    match op {
        Op::Create {
            name,
            start,
            step,
            max_offset,
            block,
        } => {
            let spec = CollectionSpec {
                start: *start,
                step: NonZeroU64::new(*step).expect("non-zero"),
                max: start + max_offset,
                durability: durability(*block),
                ..CollectionSpec::named(NAMES[*name])
            };
            (
                format!("{:?}", engine.create(spec, now).map(|c| c.last_id())),
                vec![],
            )
        }
        Op::Next { name, count, key } => {
            let request = NextRequest {
                name: NAMES[*name].into(),
                count: *count,
                idempotency_key: key.map(|k| format!("k{k}")),
            };
            match engine.next(&request, now) {
                Ok(issued) => {
                    let ids = if issued.replayed {
                        vec![]
                    } else {
                        let step = Id::try_from(issued.range.step.get()).expect("small step");
                        (0..Id::from(issued.range.count.get()))
                            .map(|i| issued.range.first + i * step)
                            .collect()
                    };
                    (format!("{:?}", (issued.range, issued.replayed)), ids)
                }
                Err(e) => (format!("{e:?}"), vec![]),
            }
        }
        Op::Reset {
            name,
            value_offset,
            allow_rewind,
        } => {
            let start = engine.get(NAMES[*name]).map_or(0, |c| c.spec().start);
            let value = value_offset.map(|o| start + o);
            (
                format!("{:?}", engine.reset(NAMES[*name], value, *allow_rewind)),
                vec![],
            )
        }
        Op::Delete { name } => (
            format!("{:?}", engine.delete(NAMES[*name]).map(|c| c.last_id())),
            vec![],
        ),
        Op::SetMax { name, max_offset } => {
            let start = engine.get(NAMES[*name]).map_or(0, |c| c.spec().start);
            let patch = CollectionPatch {
                max: Some(start + max_offset),
                ..CollectionPatch::default()
            };
            (
                format!(
                    "{:?}",
                    engine
                        .update(NAMES[*name], &patch)
                        .map(|u| u.after.spec().max)
                ),
                vec![],
            )
        }
        Op::SetBlock { name, block } => {
            let patch = CollectionPatch {
                durability: Some(durability(*block)),
                ..CollectionPatch::default()
            };
            (
                format!(
                    "{:?}",
                    engine
                        .update(NAMES[*name], &patch)
                        .map(|u| u.after.last_id())
                ),
                vec![],
            )
        }
    }
}

/// Ops after which the same IDs may legitimately be issued again.
fn allows_reissue(op: &Op) -> Option<usize> {
    match op {
        Op::Reset {
            name,
            allow_rewind: true,
            ..
        }
        | Op::Delete { name } => Some(*name),
        _ => None,
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, ..ProptestConfig::default() })]

    #[test]
    fn redb_matches_the_in_memory_model_and_never_reissues(ops in proptest::collection::vec(op(), 1..60)) {
        let dir = tempfile::tempdir().expect("temp dir");
        let durable = Engine::new(
            Arc::new(RedbStore::open(dir.path(), GroupCommitConfig::default()).expect("open")),
            Limits::default(),
        );
        let model = Engine::new(Arc::new(InMemoryStore::new()), Limits::default());
        let now = SystemTime::UNIX_EPOCH;
        let mut issued: HashMap<usize, HashSet<Id>> = HashMap::new();

        for op in &ops {
            let (got, ids) = apply(&durable, op, now);
            let (want, _) = apply(&model, op, now);
            prop_assert_eq!(&got, &want, "diverged on {:?}", op);
            if let Some(name) = allows_reissue(op) {
                issued.remove(&name);
            }
            if let Op::Next { name, .. } = op {
                let seen = issued.entry(*name).or_default();
                for id in ids {
                    prop_assert!(seen.insert(id), "ID {} issued twice for {}", id, NAMES[*name]);
                }
            }
        }
    }
}
