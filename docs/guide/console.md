# Admin console

`idbseqd` can serve a small web console for the person who runs it. It does the
admin's everyday jobs in a browser: look after collections and API keys, read
the audit log and check the server's health. Applications still use the
[HTTP API](../API.md) or an SDK; the console is for people.

## Turn it on

The console is **off by default**. Turn it on with any of these:

```bash
idbseqd --ui          # a flag
IDBSEQ_UI=true idbseqd # an environment variable
```

```toml
# idbseq.toml
ui = true
```

Then open `/ui/` on the server, for example `http://127.0.0.1:7878/ui/`, and sign
in with the [admin password](./security.md#the-admin-account).

The console is part of the `idbseqd` binary: there is nothing else to install or
run, and it loads nothing from the internet. It is a small Vue app (Tailwind CSS
and shadcn-vue components) whose built files are compiled into the server.

## On an embedded file

`idbseqd console app.idbseq` opens the same console on an
[embedded file](./embedded.md#looking-inside-a-file), on this machine only,
signed in by the link it prints. It shows collections and status; API keys, the
audit log and the account are server features and are hidden.

## First-run setup

The console can create the admin account for you. When it
starts with the console on and **no admin yet**, `idbseqd` prints a one-time link
on stderr:

```text
idbseqd: no admin account yet. Open this link within 30 minutes to choose the admin password:

http://127.0.0.1:7878/ui/#/setup?token=3f9c…
```

Open it, choose a password (at least 8 characters) and you are signed in. So a
new server needs nothing more than:

```bash
./idbseqd --ui
```

- **Only whoever can read the server's output can use it.** The token is 256
  random bits, kept only as a hash in memory.
- **It works once and expires after 30 minutes.** A rejected password does not
  use it up. Restart `idbseqd` for a new link.
- **It stays out of logs.** The token is in the URL fragment (after `#`), which
  browsers never send to the server.
- `idbseqd admin init` and `IDBSEQ_ADMIN_PASSWORD` still work, and with either
  no link is printed. With the console off, no link is printed and
  `POST /v1/auth/setup` does not exist.
- Server bound to `0.0.0.0` (as in Docker)? The link says `127.0.0.1`; use the
  host and port you reach the server on instead.

## What it does

The layout follows familiar admin consoles: an icon rail on
the left, a searchable list of collections next to it, and side panels for
settings and details.

| Screen          | What you can do                                                                                                    |
| --------------- | ------------------------------------------------------------------------------------------------------------------ |
| **Collections** | Search the list by name prefix (protected collections show a lock), and create a collection in a side panel         |
| **Collection**  | See the last and next ID, how much of the range is left and the durability; take IDs by hand (they are used up, as from an application); see its recent changes. **Settings** opens a panel to edit it, reset the counter or delete it |
| **API preview** | On a collection: each operation (generate, batch, last ID, info, update, reset, delete) with its endpoint, the scope it needs, copyable Node.js SDK code, and the exact response this collection would give now |
| **API keys**    | Create a key by ticking scopes and listing collections, copy it (shown once), see when each key was last used, revoke |
| **Audit log**   | Filter by collection and action; open any entry to see what changed, field by field, with the request ID and client IP |
| **Status**      | Check `/healthz`, `/readyz` and the API version                                                                    |
| **Account**     | Change the admin password (at the bottom of the left rail). Every admin session ends, this one included, so you sign in again with the new password |

Reset and delete ask you to **type the collection's name**, the same safeguard
as the CLI's `confirm`. A reset that would move the counter backwards also needs
**Allow rewind**, with the warning that rewinding can hand out IDs twice.

IDs are shown exactly, all the way up to 9223372036854775807; the console never
rounds 64-bit numbers.

## Security

- **Admin only.** The console signs in with the admin account; API keys cannot
  sign in to it. The page itself needs no credentials, but every call it makes
  does, exactly as from the CLI.
- **The session stays in memory.** The session token is never written to a
  cookie, `localStorage` or `sessionStorage`. Reloading or closing the tab signs
  you out.
- **Idle sign-out.** While you use the console it keeps the session alive (at
  most one `POST /v1/auth/refresh` a minute), so it ends only after 15 minutes
  without activity (`auth.session_ttl_minutes`). A minute before, it asks
  **Are you still there?**: **Stay signed in** extends the session, **Sign out**
  ends it, and no answer signs you out. The bottom of the left rail counts down
  the time left. Whatever your activity, a session ends 12 hours after sign-in.
- **Same origin, no CORS.** The console is served by the API server itself, so
  the server still sends no CORS headers and other sites still cannot call it.
- **Strict Content-Security-Policy.** Scripts, styles and requests are limited
  to the server itself, with no inline scripts or styles and no framing:
  `default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; frame-ancestors 'none'; …`.
  Names and values are always inserted as text, never as HTML.
- **Use TLS beyond localhost.** The admin password and session token cross the
  network like any other credential. Open the console only on `127.0.0.1` or
  over HTTPS ([Deployment](./deployment.md)). Behind a reverse proxy, route
  `/ui/` to idbseqd along with `/v1/`.

Turn the console on only where the admin needs it. It adds no new permissions,
but `ui = false` keeps the server's surface to the API alone.

## Compared with the CLI

The console and the [CLI](../clients/cli.md) do the same admin jobs through the
same API. The CLI is better for scripts, automation and SSH sessions; the
console is easier for occasional use and for reading the audit log. Backups,
restores and recovering a forgotten password stay with `idbseqd` on the server
host (`idbseqd backup`, `idbseqd restore`, `idbseqd admin reset-password`).
