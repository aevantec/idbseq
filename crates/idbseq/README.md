# idbseq

Embedded ID sequences in one portable file, like SQLite for counters. Each
named collection hands out increasing IDs and never the same one twice, even
after a crash. No server, no network.

```rust
use idbseq::{CollectionSpec, Database, Options};

let ids = Database::open("app.idbseq")?;
ids.create_collection(CollectionSpec::named("invoices"))?;
let id = ids.next_id("invoices")?; // 1
let batch = ids.next_ids("invoices", 10)?; // 2..=11

// Several processes taking turns on one file:
let shared = Database::open_with("app.idbseq", Options::shared())?;
```

- **Exclusive mode** (default): the handle holds the file; threads share it and
  their disk writes.
- **Shared mode**: every call opens the file, writes and closes it, waiting up
  to a busy timeout (default 5 s) for other processes.
- The file is portable, and `idbseqd` can serve it later as its data file.
- `idbseq::json` speaks the HTTP API's JSON, for bindings to other languages
  (the Node.js SDK's `IdbSeqDatabase` uses it).

Several machines need one shared sequence: run the
[idbseq server](https://aevantec.github.io/idbseq/). Guide:
[Embedded](https://aevantec.github.io/idbseq/guide/embedded).
