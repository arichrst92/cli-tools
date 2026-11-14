<p align="center">
  <img src="assets/logo-ecc.svg" alt="ECC Church" width="160">
</p>

<h1 align="center">els</h1>

<p align="center">Command line access to the ECC Core API, for macOS, Linux and Windows.<br>
For ECC developers only. Please do not share this repository outside the team.</p>

---

## What it does

- **Log in** with a personal access token or a WhatsApp code.
- **Browse the API offline.** `els modules` lists all 93 modules and 763 endpoints.
- **Manage events** with ready-made commands.
- **Call any other endpoint** with `els call`.

## 1. Get your access token

Access tokens come **only from the ECC Portal**. Nobody should send you one by chat or email.

1. Log in to the **Portal**.
2. Open **Developer Tools → API Access**.
3. Click **Create token**, give it a name (for example `els on my laptop`) and pick an expiry. Shorter is safer: 7 or 30 days is plenty for testing.
4. **Copy the token right away.** It starts with `ecc_pat_` and is shown **only once**.

What the token can do is exactly what your role can do in the Portal, nothing more.

> Don't see **API Access**? Only some IT Minister team roles can create tokens. Ask the IT Minister team to enable it for your role.
> No access at all? You can still log in with a WhatsApp code (step 3, option 2).

## 2. Install

You need **Rust** and a C compiler (one of the libraries builds a small C part).

| System  | Before installing                                                                                                                       |
|---------|-----------------------------------------------------------------------------------------------------------------------------------------|
| macOS   | `xcode-select --install`, then Rust from [rustup.rs](https://rustup.rs)                                                                  |
| Linux   | `sudo apt install build-essential git` (Debian/Ubuntu) or `sudo dnf install gcc git` (Fedora), then Rust from [rustup.rs](https://rustup.rs) |
| Windows | Run the installer from [rustup.rs](https://rustup.rs). When it asks, let it install the **Visual Studio C++ Build Tools**. Also install [Git](https://git-scm.com). |

Then, in a terminal (Terminal on macOS/Linux, PowerShell on Windows):

```bash
git clone https://github.com/kekvin1868/els-app.git
cd els-app
cargo install --path .
els --help
```

`cargo install` puts `els` in `~/.cargo/bin` (Windows: `%USERPROFILE%\.cargo\bin`), which rustup already added to your PATH. If `els` isn't found, open a new terminal.

## 3. Log in

```bash
els auth login
```

```
How do you want to log in?
  1) Personal API token  (Portal -> Developer Tools -> API Access)
  2) WhatsApp code       (sent to your phone number)
```

- **1) Personal API token:** paste the token from step 1. Nothing shows while you paste; that's normal.
- **2) WhatsApp code:** type your phone number (`0812...` or `+62812...`), then the code you receive.

Check that it worked:

```bash
els auth whoami     # Logged in as <your name> (kode ...)
els auth access     # what your role can read, write and delete
els auth status     # where your token is stored (no API call)
```

### Where your token is stored

| System  | Location                         | How it's protected                                                    |
|---------|----------------------------------|-----------------------------------------------------------------------|
| macOS   | Keychain, item `ecc-pat`         | Encrypted, unlocked with your Mac login                               |
| Linux   | `~/.config/els/ecc-pat`          | A file only your user can read (permissions `600`). Not encrypted.    |
| Windows | `%APPDATA%\els\ecc-pat`          | Inside your own user profile, which other users can't open. Not encrypted. |

On Linux and Windows the token sits in a plain file, the same way the GitHub CLI does it when no keyring is available. Anyone who can sign in as you, or who gets a copy of your user folder, can read it. That's why a **short expiry** matters, and why `els auth logout` deletes the file.

### Rather not keep it on disk? Use `ELS_TOKEN`

If `ELS_TOKEN` is set, `els` uses it and ignores the stored token. That suits shared machines, servers and CI. These commands ask for the token without showing it or saving it in your shell history:

**macOS / Linux (bash, zsh)**
```bash
read -rs ELS_TOKEN && export ELS_TOKEN    # paste, press Enter
els auth whoami
```

**Windows (PowerShell 7+; the older built-in Windows PowerShell 5 lacks `-MaskInput`)**
```powershell
$env:ELS_TOKEN = Read-Host -MaskInput "Token"
els auth whoami
```

It only lasts until you close that terminal. In CI, store the token as a secret (for example a GitHub Actions secret) and expose it as `ELS_TOKEN`.

## 4. Use it

### Explore the API

```bash
els modules                 # every module, with how many endpoints it has
els modules event           # every endpoint in one module
els modules -s qris         # search all endpoints
```

### Events

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

> **Windows tip:** PowerShell can mangle quotes inside inline JSON. Put the body in a file and pass the file name instead: `els event update <id> body.json`.

### Any other endpoint

```bash
els call get /admin/homecell -q limit=20
els call patch /admin/event/<id>/peserta/<participationId> body.json
```

Fill in the `:params` from `els modules` with real ids.

## Safety

- Deletes and anything under `/maintenance` ask you to type `yes`. In scripts, pass `--yes` on purpose.
- Your token has the same power as your role. Treat it like a password: never paste it in chat, tickets, issues or code.
- `els auth logout` removes the token from your computer. The token itself keeps working until it expires, so if it might have leaked, **revoke it in the Portal under Developer Tools → API Access**.

## Settings

| Variable    | What it does                                             |
|-------------|----------------------------------------------------------|
| `ELS_TOKEN` | Use this token instead of the stored one                 |
| `ELS_URL`   | Point `els` at another server, for example a test server |

## Updating the endpoint list

`src/catalog.tsv` is generated from the API reference PDF in the Portal. When a new one comes out:

```bash
python3 tools/pdftext.py ecc-api-reference.pdf > ecc.txt
python3 tools/mkcat.py ecc.txt src/catalog.tsv
cargo test
```

Don't commit the PDF; `.gitignore` already skips it.

## Known gaps

- **Not tested against the live API yet:** WhatsApp login, and refreshing its token.
- **Linux and Windows** are built and tested automatically on every push, but nobody has used `els` on them by hand yet.
- Image uploads (event hero image, QRIS, transfer receipts) aren't supported yet.

Found a problem? Open an issue with the command and the error message. Never include your token.
