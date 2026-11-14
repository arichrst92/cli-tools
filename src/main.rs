mod client;

use std::io::{self, IsTerminal, Read, Write};
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use serde_json::{Value, json};

use client::{
    Client, REFRESH_ITEM, TOKEN_ITEM, base_url, find_tokens, keychain_delete, keychain_get,
    keychain_set, stored_token,
};

/// Every endpoint in the API reference, one per line: module, method, path, summary.
/// Regenerate with tools/ when a new PDF comes out.
const CATALOG: &str = include_str!("catalog.tsv");

#[derive(Parser)]
#[command(
    name = "els",
    version,
    about = "CLI for the ECC Core API",
    after_help = "Start here:\n  els auth login        log in\n  els modules           see every module the API has\n  els modules event     see one module's endpoints\n  els event list        list events"
)]
struct Cli {
    /// Print the full JSON answer instead of a short table
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Log in, see who you are and what your role can do, log out
    #[command(subcommand)]
    Auth(AuthCommand),
    /// Browse the API offline: every module and endpoint from the API reference
    Modules {
        /// Module to open, e.g. event, me/homecell or /admin/event
        module: Option<String>,
        /// Only endpoints whose path or description contains this text
        #[arg(long, short)]
        search: Option<String>,
    },
    /// Events, Movement -> Event in the Portal
    #[command(subcommand)]
    Event(EventCommand),
    /// Escape hatch: call any endpoint, e.g. els call get /admin/event/<id>/peserta
    Call {
        /// get, post, put, patch or delete
        method: String,
        /// Path with the :params filled in, as listed by: els modules
        path: String,
        /// Body as JSON, a path to a .json file, or - to read stdin
        #[arg(value_name = "JSON")]
        body: Option<String>,
        /// Query parameter, repeat for more: -q page=2 -q limit=50
        #[arg(long, short)]
        query: Vec<String>,
        /// Skip the confirmation for deletes and maintenance endpoints
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Subcommand)]
enum AuthCommand {
    /// Log in and keep the token in the macOS Keychain
    Login {
        /// Skip the menu and log in this way
        #[arg(long, value_enum)]
        with: Option<LoginMethod>,
        /// Phone number for a WhatsApp login, e.g. +6281234567890 or 081234567890
        #[arg(long)]
        phone: Option<String>,
    },
    /// Show where the token comes from, without calling the API
    Status,
    /// Ask the API who the token belongs to
    Whoami,
    /// Show which Portal menus your role can read, write and delete
    Access,
    /// Remove the token from the Keychain
    Logout {
        /// After a WhatsApp login, also end your sessions on every other device
        #[arg(long)]
        all: bool,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum LoginMethod {
    /// Personal API token from Portal -> Developer Tools -> API Access
    Token,
    /// One time code sent to your phone by WhatsApp
    Whatsapp,
}

#[derive(Subcommand)]
enum EventCommand {
    /// List events
    List {
        #[arg(long)]
        page: Option<u32>,
        /// Rows per page, at most 500
        #[arg(long)]
        limit: Option<u32>,
        #[arg(long, short)]
        search: Option<String>,
        /// Any other query parameter: -q key=value
        #[arg(long, short)]
        query: Vec<String>,
    },
    /// Show one event, by id or slug
    Get { id_or_slug: String },
    /// Create an event from a JSON body
    Create {
        /// JSON, a path to a .json file, or - to read stdin
        #[arg(value_name = "JSON")]
        body: String,
    },
    /// Change an event; send only the fields to change
    Update {
        id: String,
        /// JSON, a path to a .json file, or - to read stdin
        #[arg(value_name = "JSON")]
        body: String,
    },
    /// Delete an event, with its participants, hero image and QRIS
    Delete {
        id: String,
        /// Skip the confirmation
        #[arg(long)]
        yes: bool,
    },
    /// Copy an event into a new unpublished draft (no participants or donations)
    Duplicate { id: String },
    /// List an event's participants
    Peserta { id: String },
    /// Attendance counts by status
    Stats { id: String },
    /// List an event's donations
    Donations {
        id: String,
        /// BAYAR, MENUNGGU_VERIFIKASI or BATAL
        #[arg(long)]
        status: Option<String>,
    },
    /// List the ministries and volunteers serving at an event
    Pelayanan { id: String },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), String> {
    let as_json = cli.json;
    match cli.command {
        Command::Auth(command) => auth(command, as_json),
        Command::Modules { module, search } => modules(module, search),
        Command::Event(command) => event(command, as_json),
        Command::Call {
            method,
            path,
            body,
            query,
            yes,
        } => {
            if !path.starts_with('/') {
                return Err(format!("the path starts with /, e.g. /{path}"));
            }
            if let Some(param) = path.split('/').find(|part| part.starts_with(':')) {
                return Err(format!("fill in {param} in the path first"));
            }
            let method = method.to_uppercase();
            if method == "DELETE" || path.contains("/maintenance") {
                confirm(&format!("{method} {path}"), yes)?;
            }
            let body = body.map(|raw| read_json(&raw)).transpose()?;
            let answer = Client::logged_in()?.send(&method, &path, &parse_query(&query)?, body)?;
            print_json(&answer);
            Ok(())
        }
    }
}

fn auth(command: AuthCommand, as_json: bool) -> Result<(), String> {
    match command {
        AuthCommand::Login { with, phone } => {
            let method = match with {
                Some(method) => method,
                None if phone.is_some() => LoginMethod::Whatsapp,
                None => login_menu()?,
            };
            match method {
                LoginMethod::Token => login_token()?,
                LoginMethod::Whatsapp => login_whatsapp(phone)?,
            }
            whoami(false)
        }
        AuthCommand::Status => {
            println!("API:   {}", base_url());
            match stored_token() {
                None => println!("Token: none, run: els auth login"),
                Some((token, source)) => {
                    let kind = if token.starts_with("ecc_pat_") {
                        "personal API token"
                    } else if keychain_get(REFRESH_ITEM).is_some() {
                        "WhatsApp login (refreshes itself)"
                    } else {
                        "bearer token"
                    };
                    let shown: String = token.chars().take(12).collect();
                    println!("Token: {shown}... ({kind})");
                    println!("From:  {source}");
                }
            }
            Ok(())
        }
        AuthCommand::Whoami => whoami(as_json),
        AuthCommand::Access => {
            let answer = Client::logged_in()?.get("/auth/me/access", &[])?;
            if as_json {
                print_json(&answer);
                return Ok(());
            }
            let data = &answer["data"];
            println!("Portal access: {}", yes_no(data["canAccessPortal"].as_bool()));
            println!("\n{:<28} READ  WRITE DELETE", "MENU");
            if let Some(menus) = data["menuAccess"].as_object() {
                for (menu, can) in menus {
                    println!(
                        "{menu:<28} {:<5} {:<5} {}",
                        yes_no(can["canRead"].as_bool()),
                        yes_no(can["canWrite"].as_bool()),
                        yes_no(can["canDelete"].as_bool())
                    );
                }
            }
            Ok(())
        }
        AuthCommand::Logout { all } => {
            if let Some(refresh) = keychain_get(REFRESH_ITEM) {
                let path = if all { "/auth/logout?all=true" } else { "/auth/logout" };
                // Best effort: the local copy goes either way.
                if let Ok(mut client) = Client::logged_in() {
                    let _ = client.post(path, json!({ "refreshToken": refresh }));
                }
            }
            let was_token = keychain_get(TOKEN_ITEM).is_some_and(|t| t.starts_with("ecc_pat_"));
            keychain_delete(TOKEN_ITEM);
            keychain_delete(REFRESH_ITEM);
            println!("Logged out: removed the token from the Keychain.");
            if was_token {
                println!(
                    "The personal token itself still works until it expires. To kill it, revoke it in Portal -> Developer Tools -> API Access."
                );
            }
            if std::env::var("ELS_TOKEN").is_ok() {
                println!("ELS_TOKEN is still set in this shell, unset it too.");
            }
            Ok(())
        }
    }
}

fn login_menu() -> Result<LoginMethod, String> {
    eprintln!("How do you want to log in?");
    eprintln!("  1) Personal API token  (Portal -> Developer Tools -> API Access)");
    eprintln!("  2) WhatsApp code       (sent to your phone number)");
    match prompt("Choose 1 or 2: ")?.as_str() {
        "1" => Ok(LoginMethod::Token),
        "2" => Ok(LoginMethod::Whatsapp),
        other => Err(format!("{other:?} is not 1 or 2")),
    }
}

fn login_token() -> Result<(), String> {
    eprintln!("Paste your personal API token. What you paste stays hidden.");
    let token = rpassword::prompt_password("Token: ")
        .map_err(|e| format!("could not read the token: {e}"))?
        .trim()
        .to_string();
    if !token.starts_with("ecc_pat_") {
        eprintln!("warning: personal tokens start with ecc_pat_, this one does not");
    }
    keychain_set(TOKEN_ITEM, &token)?;
    keychain_delete(REFRESH_ITEM);
    Ok(())
}

fn login_whatsapp(phone: Option<String>) -> Result<(), String> {
    let phone = match phone {
        Some(phone) => phone,
        None => prompt("Phone number (+62...): ")?,
    };
    let phone = normalize_phone(&phone)?;
    let mut client = Client::anonymous();
    client.post(
        "/auth/otp/request",
        json!({ "noHp": phone, "purpose": "LOGIN" }),
    )?;
    eprintln!("A code was sent by WhatsApp to {phone}.");
    let code = prompt("Code: ")?;
    let answer = client.post(
        "/auth/otp/verify",
        json!({ "noHp": phone, "kode": code, "purpose": "LOGIN" }),
    )?;
    let (access, refresh) = find_tokens(&answer)
        .ok_or("the login answered without a token; rerun with --json on a call to see the raw answer")?;
    keychain_set(TOKEN_ITEM, &access)?;
    match refresh {
        Some(refresh) => keychain_set(REFRESH_ITEM, &refresh)?,
        None => keychain_delete(REFRESH_ITEM),
    }
    Ok(())
}

/// The API wants E.164: +62 followed by the number without its leading 0.
fn normalize_phone(raw: &str) -> Result<String, String> {
    let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
    let phone = if let Some(rest) = digits.strip_prefix('0') {
        format!("+62{rest}")
    } else {
        format!("+{digits}")
    };
    if phone.len() < 10 || phone.len() > 16 {
        return Err(format!("{raw:?} does not look like a phone number"));
    }
    Ok(phone)
}

fn whoami(as_json: bool) -> Result<(), String> {
    let answer = Client::logged_in()?.get("/admin/me", &[])?;
    if as_json {
        print_json(&answer);
        return Ok(());
    }
    let me = &answer["data"];
    println!(
        "Logged in as {} (kode {})",
        me["namaLengkap"].as_str().unwrap_or("?"),
        me["kode"].as_str().unwrap_or("?")
    );
    Ok(())
}

fn event(command: EventCommand, as_json: bool) -> Result<(), String> {
    let mut client = Client::logged_in()?;
    let answer = match command {
        EventCommand::List {
            page,
            limit,
            search,
            query,
        } => {
            let mut params = parse_query(&query)?;
            for (key, value) in [
                ("page", page.map(|p| p.to_string())),
                ("limit", limit.map(|l| l.to_string())),
                ("search", search),
            ] {
                if let Some(value) = value {
                    params.push((key.to_string(), value));
                }
            }
            let answer = client.get("/admin/event", &params)?;
            if !as_json {
                print_event_table(&answer);
                return Ok(());
            }
            answer
        }
        EventCommand::Get { id_or_slug } => {
            client.get(&format!("/admin/event/{}", segment(&id_or_slug)?), &[])?
        }
        EventCommand::Create { body } => client.post("/admin/event", read_json(&body)?)?,
        EventCommand::Update { id, body } => client.send(
            "PATCH",
            &format!("/admin/event/{}", segment(&id)?),
            &[],
            Some(read_json(&body)?),
        )?,
        EventCommand::Delete { id, yes } => {
            let path = format!("/admin/event/{}", segment(&id)?);
            confirm(&format!("delete event {id} with all its participants"), yes)?;
            client.send("DELETE", &path, &[], None)?
        }
        EventCommand::Duplicate { id } => {
            client.post(&format!("/admin/event/{}/duplicate", segment(&id)?), json!({}))?
        }
        EventCommand::Peserta { id } => {
            client.get(&format!("/admin/event/{}/peserta", segment(&id)?), &[])?
        }
        EventCommand::Stats { id } => {
            client.get(&format!("/admin/event/{}/checkin/stats", segment(&id)?), &[])?
        }
        EventCommand::Donations { id, status } => {
            let query: Vec<(String, String)> =
                status.into_iter().map(|s| ("status".into(), s)).collect();
            client.get(&format!("/admin/event/{}/donations", segment(&id)?), &query)?
        }
        EventCommand::Pelayanan { id } => {
            client.get(&format!("/admin/event/{}/pelayanan", segment(&id)?), &[])?
        }
    };
    print_json(&answer);
    Ok(())
}

fn print_event_table(answer: &Value) {
    let empty = Vec::new();
    let events = answer["data"].as_array().unwrap_or(&empty);
    println!("{:<36}  {:<10}  TITLE", "ID", "START");
    for event in events {
        let start = event["tanggalMulai"].as_str().unwrap_or("");
        println!(
            "{:<36}  {:<10}  {}",
            event["id"].as_str().unwrap_or(""),
            start.get(..10).unwrap_or(start),
            event["judul"].as_str().unwrap_or("")
        );
    }
    let meta = &answer["meta"];
    if !meta.is_null() {
        println!(
            "\npage {} of {}, {} events in total",
            meta["page"], meta["totalPages"], meta["total"]
        );
    }
}

fn modules(module: Option<String>, search: Option<String>) -> Result<(), String> {
    let rows: Vec<[&str; 4]> = CATALOG
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(4, '\t');
            Some([parts.next()?, parts.next()?, parts.next()?, parts.next().unwrap_or("")])
        })
        .collect();

    if let Some(text) = search {
        let text = text.to_lowercase();
        let hits: Vec<_> = rows
            .iter()
            .filter(|row| {
                module.as_deref().is_none_or(|m| row[0].contains(m))
                    && (row[2].to_lowercase().contains(&text) || row[3].to_lowercase().contains(&text))
            })
            .collect();
        print_endpoints(&hits);
        return Ok(());
    }

    let Some(module) = module else {
        let mut names: Vec<(&str, usize)> = Vec::new();
        for row in &rows {
            match names.last_mut() {
                Some((name, count)) if *name == row[0] => *count += 1,
                _ => names.push((row[0], 1)),
            }
        }
        for (name, count) in &names {
            println!("{name:<40} {count:>3}");
        }
        println!(
            "\n{} endpoints in {} modules. Open one with: els modules event",
            rows.len(),
            names.len()
        );
        return Ok(());
    };

    let wanted = if module.starts_with('/') {
        vec![module.clone()]
    } else {
        vec![format!("/admin/{module}"), format!("/{module}")]
    };
    let hits: Vec<_> = rows.iter().filter(|row| wanted.iter().any(|w| row[0] == w)).collect();
    if !hits.is_empty() {
        print_endpoints(&hits);
        return Ok(());
    }
    let mut close: Vec<&str> = rows
        .iter()
        .map(|row| row[0])
        .filter(|name| name.contains(module.as_str()))
        .collect();
    close.dedup();
    if close.is_empty() {
        return Err(format!("no module called {module}, list them with: els modules"));
    }
    println!("No module called {module}. Did you mean:");
    for name in close {
        println!("  {name}");
    }
    Ok(())
}

fn print_endpoints(rows: &[&[&str; 4]]) {
    let width = rows.iter().map(|row| row[2].len()).max().unwrap_or(0);
    for row in rows {
        println!("{:<6} {:<width$}  {}", row[1], row[2], row[3]);
    }
    println!("\nCall one with: els call <method> <path>   (fill in the :params)");
}

/// An id or slug goes into the path, so it must not be able to walk to another endpoint.
fn segment(value: &str) -> Result<&str, String> {
    if value.is_empty()
        || !value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(format!("{value:?} is not an id or slug"));
    }
    Ok(value)
}

fn parse_query(pairs: &[String]) -> Result<Vec<(String, String)>, String> {
    pairs
        .iter()
        .map(|pair| {
            pair.split_once('=')
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .ok_or_else(|| format!("{pair:?} is not key=value"))
        })
        .collect()
}

/// Inline JSON, - for stdin, or a path to a file.
fn read_json(raw: &str) -> Result<Value, String> {
    let text = if raw == "-" {
        let mut text = String::new();
        io::stdin()
            .read_to_string(&mut text)
            .map_err(|e| format!("could not read stdin: {e}"))?;
        text
    } else if raw.trim_start().starts_with(['{', '[']) {
        raw.to_string()
    } else {
        std::fs::read_to_string(raw).map_err(|e| format!("could not read {raw}: {e}"))?
    };
    serde_json::from_str(&text).map_err(|e| format!("the body is not valid JSON: {e}"))
}

/// Without a terminal to ask on, only --yes lets it through.
fn confirm(what: &str, yes: bool) -> Result<(), String> {
    if yes {
        return Ok(());
    }
    if !io::stdin().is_terminal() {
        return Err(format!("refusing to {what} without --yes"));
    }
    let answer = prompt(&format!("About to {what}. Type yes to go on: "))?;
    if answer != "yes" {
        return Err("stopped, nothing was changed".into());
    }
    Ok(())
}

fn prompt(label: &str) -> Result<String, String> {
    eprint!("{label}");
    io::stderr().flush().ok();
    let mut line = String::new();
    io::stdin()
        .read_line(&mut line)
        .map_err(|e| format!("could not read the answer: {e}"))?;
    Ok(line.trim().to_string())
}

fn print_json(value: &Value) {
    println!("{}", serde_json::to_string_pretty(value).unwrap());
}

fn yes_no(value: Option<bool>) -> &'static str {
    if value == Some(true) { "yes" } else { "no" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phone_numbers_become_e164() {
        assert_eq!(normalize_phone("0812-3456-7890").unwrap(), "+6281234567890");
        assert_eq!(normalize_phone("+62 812 3456 7890").unwrap(), "+6281234567890");
        assert!(normalize_phone("123").is_err());
    }

    #[test]
    fn ids_cannot_leave_their_path_segment() {
        assert!(segment("d919b4a8-3276-4445-9b47-3a28e1962317").is_ok());
        assert!(segment("retreat_2026").is_ok());
        assert!(segment("../maintenance/db").is_err());
        assert!(segment("x?all=true").is_err());
        assert!(segment("").is_err());
    }

    #[test]
    fn tokens_are_found_where_the_api_puts_them() {
        let nested = json!({ "data": { "accessToken": "a", "refreshToken": "r" } });
        assert_eq!(find_tokens(&nested), Some(("a".into(), Some("r".into()))));
        let tokens = json!({ "data": { "tokens": { "accessToken": "a" } } });
        assert_eq!(find_tokens(&tokens), Some(("a".into(), None)));
        assert_eq!(find_tokens(&json!({ "data": {} })), None);
    }

    #[test]
    fn catalog_has_every_event_endpoint() {
        let events = CATALOG.lines().filter(|l| l.starts_with("/admin/event\t")).count();
        assert_eq!(events, 38);
    }
}
