use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{Value, json};

pub const DEFAULT_URL: &str = "https://api.eccchurch.global";
/// Keychain item holding the bearer token: a personal token (ecc_pat_...) or the
/// JWT from a WhatsApp login. Named ecc-pat because that is where it was first stored.
pub const TOKEN_ITEM: &str = "ecc-pat";
/// Only present after a WhatsApp login. Personal tokens have no refresh.
pub const REFRESH_ITEM: &str = "els-refresh";

pub fn base_url() -> String {
    std::env::var("ELS_URL")
        .ok()
        .filter(|url| !url.is_empty())
        .unwrap_or_else(|| DEFAULT_URL.to_string())
        .trim_end_matches('/')
        .to_string()
}

// Where tokens live: the macOS Keychain, or a file only you can read elsewhere.
// ponytail: plain file outside macOS, the same fallback `gh` uses without a keyring.
// Owner-only (0600) on Linux; Windows relies on the per-user AppData folder.
// Move to the keyring crate (Credential Manager, Secret Service) if that is not enough.
const ON_MAC: bool = cfg!(target_os = "macos");

pub fn secret_get(item: &str) -> Option<String> {
    if ON_MAC {
        keychain_get(item)
    } else {
        file_get(&secret_dir().ok()?, item)
    }
}

pub fn secret_set(item: &str, secret: &str) -> Result<(), String> {
    // Tokens and JWTs only use these characters. Anything else would break the
    // Keychain quoting, and usually means the paste picked up something extra.
    if secret.is_empty()
        || !secret
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
    {
        return Err("the token has unexpected characters; paste only the token itself".into());
    }
    if ON_MAC {
        keychain_set(item, secret)
    } else {
        file_set(&secret_dir()?, item, secret)
    }
}

pub fn secret_delete(item: &str) {
    if ON_MAC {
        keychain_delete(item);
    } else if let Ok(dir) = secret_dir() {
        let _ = std::fs::remove_file(dir.join(item));
    }
}

/// Where a token is kept, for `els auth status`.
pub fn secret_place(item: &str) -> String {
    match secret_dir() {
        _ if ON_MAC => format!("macOS Keychain item \"{item}\""),
        Ok(dir) => dir.join(item).display().to_string(),
        Err(e) => e,
    }
}

/// Windows: %APPDATA%\els. Elsewhere: $XDG_CONFIG_HOME/els or ~/.config/els.
pub fn secret_dir() -> Result<PathBuf, String> {
    let var = |name: &str| std::env::var(name).ok().filter(|v| !v.is_empty());
    if cfg!(windows) {
        let appdata = var("APPDATA").ok_or("APPDATA is not set")?;
        return Ok(PathBuf::from(appdata).join("els"));
    }
    let base = match var("XDG_CONFIG_HOME") {
        Some(dir) => PathBuf::from(dir),
        None => PathBuf::from(var("HOME").ok_or("HOME is not set")?).join(".config"),
    };
    Ok(base.join("els"))
}

fn file_get(dir: &Path, item: &str) -> Option<String> {
    let secret = std::fs::read_to_string(dir.join(item)).ok()?.trim().to_string();
    (!secret.is_empty()).then_some(secret)
}

fn file_set(dir: &Path, item: &str, secret: &str) -> Result<(), String> {
    let path = dir.join(item);
    let failed = |e: std::io::Error| format!("could not save {}: {e}", path.display());

    #[cfg(unix)]
    {
        use std::fs::{DirBuilder, OpenOptions, Permissions, set_permissions};
        use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};

        DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
            .map_err(failed)?;
        // Created owner-only, so the token is never readable by others, even briefly.
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&path)
            .map_err(failed)?;
        // Tighten a file an older version or the user left looser.
        set_permissions(&path, Permissions::from_mode(0o600)).map_err(failed)?;
        file.write_all(secret.as_bytes()).map_err(failed)?;
    }

    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(dir).map_err(failed)?;
        std::fs::write(&path, secret).map_err(failed)?;
    }

    Ok(())
}

fn keychain_get(item: &str) -> Option<String> {
    let out = Command::new("security")
        .args(["find-generic-password", "-s", item, "-w"])
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let secret = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (!secret.is_empty()).then_some(secret)
}

fn keychain_set(item: &str, secret: &str) -> Result<(), String> {
    let account = std::env::var("USER").unwrap_or_else(|_| "els".into());
    // Sent through stdin of `security -i`, so the secret never shows in the process list.
    let mut child = Command::new("security")
        .arg("-i")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not run the macOS security tool: {e}"))?;
    writeln!(
        child.stdin.take().unwrap(),
        "add-generic-password -U -s \"{item}\" -a \"{account}\" -w \"{secret}\""
    )
    .map_err(|e| format!("could not talk to the security tool: {e}"))?;
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    // `security -i` can exit 0 on a failed command, so read it back to be sure.
    if keychain_get(item).as_deref() != Some(secret) {
        let why = String::from_utf8_lossy(&out.stderr);
        return Err(format!("could not save to the Keychain: {}", why.trim()));
    }
    Ok(())
}

fn keychain_delete(item: &str) {
    // Loop in case an older login left a second item under the same name.
    for _ in 0..5 {
        let gone = Command::new("security")
            .args(["delete-generic-password", "-s", item])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| !status.success())
            .unwrap_or(true);
        if gone {
            break;
        }
    }
}

/// ELS_TOKEN wins, so scripts and CI can skip the stored token.
pub fn stored_token() -> Option<(String, String)> {
    if let Ok(token) = std::env::var("ELS_TOKEN")
        && !token.is_empty()
    {
        return Some((token, "ELS_TOKEN environment variable".into()));
    }
    secret_get(TOKEN_ITEM).map(|token| (token, secret_place(TOKEN_ITEM)))
}

/// Pull the token pair out of a login or refresh answer.
// ponytail: the docs name the schemas but not the fields. This accepts the usual
// spellings under data, data.tokens or the top level; pin it down once a real
// answer has been seen.
pub fn find_tokens(answer: &Value) -> Option<(String, Option<String>)> {
    let places = [&answer["data"]["tokens"], &answer["data"], answer];
    for place in places {
        let access = ["accessToken", "access_token", "token"]
            .iter()
            .find_map(|key| place[key].as_str());
        if let Some(access) = access {
            let refresh = ["refreshToken", "refresh_token"]
                .iter()
                .find_map(|key| place[key].as_str())
                .map(str::to_string);
            return Some((access.to_string(), refresh));
        }
    }
    None
}

pub struct Client {
    url: String,
    token: Option<String>,
    http: reqwest::blocking::Client,
}

impl Client {
    /// For the calls made before there is a token: OTP request and verify.
    pub fn anonymous() -> Client {
        Client {
            url: base_url(),
            token: None,
            http: reqwest::blocking::Client::new(),
        }
    }

    pub fn logged_in() -> Result<Client, String> {
        let (token, _) = stored_token().ok_or("not logged in, run: els auth login")?;
        Ok(Client {
            token: Some(token),
            ..Client::anonymous()
        })
    }

    pub fn get(&mut self, path: &str, query: &[(String, String)]) -> Result<Value, String> {
        self.send("GET", path, query, None)
    }

    pub fn post(&mut self, path: &str, body: Value) -> Result<Value, String> {
        self.send("POST", path, &[], Some(body))
    }

    /// Any method, any path. A 401 after a WhatsApp login refreshes once and retries.
    pub fn send(
        &mut self,
        method: &str,
        path: &str,
        query: &[(String, String)],
        body: Option<Value>,
    ) -> Result<Value, String> {
        let (status, answer) = self.raw(method, path, query, body.as_ref())?;
        if status == 401 && std::env::var("ELS_TOKEN").is_err() && self.refresh().is_ok() {
            let (status, answer) = self.raw(method, path, query, body.as_ref())?;
            return check(status, answer);
        }
        check(status, answer)
    }

    fn raw(
        &self,
        method: &str,
        path: &str,
        query: &[(String, String)],
        body: Option<&Value>,
    ) -> Result<(u16, Value), String> {
        let method = reqwest::Method::from_bytes(method.to_uppercase().as_bytes())
            .map_err(|_| format!("not an HTTP method: {method}"))?;
        let mut request = self
            .http
            .request(method, format!("{}{path}", self.url))
            .query(query);
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        if let Some(body) = body {
            request = request.json(body);
        }
        let response = request
            .send()
            .map_err(|e| format!("could not reach {}: {e}", self.url))?;
        let status = response.status().as_u16();
        let text = response.text().unwrap_or_default();
        let answer = if text.trim().is_empty() {
            Value::Null
        } else {
            serde_json::from_str(&text).unwrap_or(Value::String(text))
        };
        Ok((status, answer))
    }

    fn refresh(&mut self) -> Result<(), String> {
        let refresh = secret_get(REFRESH_ITEM).ok_or("no refresh token")?;
        let (status, answer) = self.raw(
            "POST",
            "/auth/refresh",
            &[],
            Some(&json!({ "refreshToken": refresh })),
        )?;
        let answer = check(status, answer)?;
        let (access, new_refresh) = find_tokens(&answer).ok_or("refresh gave no token")?;
        secret_set(TOKEN_ITEM, &access)?;
        // Rotation: the old refresh token is revoked server side, so keep the new one.
        if let Some(new_refresh) = new_refresh {
            secret_set(REFRESH_ITEM, &new_refresh)?;
        }
        self.token = Some(access);
        Ok(())
    }
}

/// The API answers { success, data, meta } or { success: false, error: { code, message, details } }.
fn check(status: u16, answer: Value) -> Result<Value, String> {
    if (200..300).contains(&status) {
        return Ok(answer);
    }
    let error = &answer["error"];
    let mut message = format!(
        "HTTP {status} {}: {}",
        error["code"].as_str().unwrap_or(""),
        error["message"].as_str().unwrap_or("")
    );
    if error.is_null() && !answer.is_null() {
        message.push_str(&answer.to_string());
    }
    if !error["details"].is_null() {
        message.push_str(&format!("\ndetails: {}", error["details"]));
    }
    match status {
        401 => message.push_str("\nhint: the token is wrong or expired, run: els auth login"),
        403 => message.push_str("\nhint: your role has no access to this menu, see: els auth access"),
        _ => {}
    }
    Err(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_store_round_trips_and_is_owner_only() {
        let dir = std::env::temp_dir().join(format!("els-test-{}", std::process::id()));
        file_set(&dir, "ecc-pat", "ecc_pat_abc").unwrap();
        file_set(&dir, "ecc-pat", "ecc_pat_new").unwrap(); // overwrite, not append
        assert_eq!(file_get(&dir, "ecc-pat").as_deref(), Some("ecc_pat_new"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(&dir.join("ecc-pat")), 0o600);
            assert_eq!(mode(&dir), 0o700);
        }
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(file_get(&dir, "ecc-pat"), None);
    }
}
