# idbseq-core

The engine behind [idbseq](https://crates.io/crates/idbseq): the collection
model and its validation, the sequence engine that never issues the same ID
twice, and storage in a [redb](https://crates.io/crates/redb) file.

Most programs want the [`idbseq`](https://crates.io/crates/idbseq) crate
instead: it opens a file and hands out IDs. This crate is shared by `idbseq`,
the `idbseqd` server and the Node.js binding, and its API changes with theirs.

Documentation: [aevantec.github.io/idbseq](https://aevantec.github.io/idbseq/).
