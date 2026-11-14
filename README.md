<p align="center">
  <img src="assets/logo-ecc.svg" alt="ECC Church" width="160">
</p>

<h1 align="center">els</h1>

<p align="center">Command line access to the ECC Core API (<code>api.eccchurch.global</code>).<br>
For ECC developers only. Please do not share this repository outside the team.</p>

---

## What it does

- **Log in** with a personal API token or a WhatsApp code. The token is kept in the macOS Keychain, never in a file.
- **Browse the API offline.** `els modules` lists all 93 modules and 763 endpoints from the API reference.
- **Manage events** with ready-made commands.
- **Call any other endpoint** with `els call`.

## Requirements

- macOS (the token is stored in the Keychain; on other systems set `ELS_TOKEN`, see below)
- Rust, from [rustup.rs](https://rustup.rs)
- An ECC account. A personal API token needs a role on the IT Minister team.

## Install

```bash
git clone https://github.com/kekvin1868/els-app.git
cd els-app
cargo install --path .
els --help
```

## Log in

```bash
els auth login
```

You get a menu:

```
How do you want to log in?
  1) Personal API token  (Portal -> Developer Tools -> API Access)
  2) WhatsApp code       (sent to your phone number)
```

**1) Personal API token**: create one in the Portal under **Developer Tools → API Access**, copy it, and paste it at the prompt. Nothing shows while you paste; that's normal. Pick a short expiry (for example 7 or 30 days).

**2) WhatsApp code**: enter your phone number (`0812...` or `+62812...`), then the code you receive.

Check it worked:

```bash
els auth whoami     # Logged in as <your name> (kode ...)
els auth access     # what your role can read, write and delete
els auth status     # where the token is stored, without calling the API
```

## Explore the API

```bash
els modules                 # every module, with how many endpoints it has
els modules event           # every endpoint in one module
els modules -s qris         # search all endpoints
```

## Events

```bash
els event list                         # table: id, start date, title
els event list --search retreat --limit 50
els event get <id-or-slug>
els event peserta <id>                 # participants
els event stats <id>                   # attendance counts
els event donations <id> --status BAYAR
els event pelayanan <id>               # ministries and volunteers

els event create event.json            # body from a file, inline JSON, or - for stdin
els event update <id> '{"judul": "New title"}'
els event duplicate <id>               # copy as an unpublished draft
els event delete <id>                  # asks you to type yes
```

Add `--json` to any command to get the full API answer instead of a table.

## Any other endpoint

```bash
els call get /admin/homecell -q limit=20
els call patch /admin/event/<id>/peserta/<participationId> '{"status": "HADIR"}'
```

Fill in the `:params` from `els modules` with real ids.

## Safety

- Deletes and anything under `/maintenance` ask you to type `yes`. In scripts, pass `--yes` on purpose.
- Your token has the same power as your role in the Portal. Treat it like a password: never paste it in chat, tickets or code.
- `els auth logout` removes the token from your Mac. A personal token keeps working until it expires, so revoke it in **Developer Tools → API Access** if it might have leaked.

## Settings

| Variable    | What it does                                                    |
|-------------|-----------------------------------------------------------------|
| `ELS_TOKEN` | Use this token instead of the Keychain (CI, Linux, Windows)     |
| `ELS_URL`   | Point at another server, default `https://api.eccchurch.global` |

## Updating the endpoint list

The endpoint list in `src/catalog.tsv` is generated from the API reference PDF (Portal → Developer Tools). When a new PDF comes out:

```bash
python3 tools/pdftext.py ecc-api-reference.pdf > /tmp/ecc.txt
python3 tools/mkcat.py /tmp/ecc.txt src/catalog.tsv
cargo test
```

Don't commit the PDF itself; `.gitignore` already skips it.

## Known gaps

- **Not tested against the live API yet:** WhatsApp login, and refreshing its token. If either fails, please open an issue with the error message (never the token).
- Image uploads (event hero image, QRIS, transfer receipts) aren't supported yet.
