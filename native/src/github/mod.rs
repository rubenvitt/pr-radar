//! GitHub-GraphQL-Client: Token-Suche, Anmeldung, Requests, Dashboard-Abfrage, Mutationen.

mod normalize;
pub mod oauth;
mod queries;

use std::collections::HashMap;
use std::future::Future;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result, anyhow, bail};
use chrono::{DateTime, Utc};
use futures::AsyncReadExt as _;
use gpui_kit::http_client::{AsyncBody, HttpClient, Method, Request};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::model::{MergeMethod, RepoInfo, Snapshot};
use normalize::RawRepo;

const ENDPOINT: &str = "https://api.github.com/graphql";
/// Repos pro GraphQL-Request
const CHUNK: usize = 8;
/// Höchstdauer eines Requests samt Body-Lesen (wie `timeoutInterval` in der iOS-App).
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenSource {
    /// Angemeldet per Device Flow („Mit GitHub anmelden“)
    OAuth,
    Env,
    GhCli,
}

impl TokenSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::OAuth => tr!("GitHub sign-in", "GitHub-Anmeldung"),
            Self::Env => "GITHUB_TOKEN",
            Self::GhCli => "GitHub CLI (gh auth token)",
        }
    }
}

#[derive(Clone)]
struct Token {
    value: String,
    source: TokenSource,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RateLimit {
    pub remaining: u64,
    pub limit: u64,
    pub reset_at: Option<DateTime<Utc>>,
    /// Punkte, die der letzte Poll gekostet hat (Summe über alle Requests)
    pub cost: u64,
}

#[derive(Deserialize, Debug, Clone)]
pub struct GqlError {
    pub message: String,
    #[serde(default)]
    pub path: Vec<Value>,
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
}

#[derive(Deserialize, Debug)]
struct GqlResponse {
    data: Option<HashMap<String, Value>>,
    #[serde(default)]
    errors: Vec<GqlError>,
}

/// GitHub drosselt: Kontingent aufgebraucht (primär) oder zu viele Anfragen (sekundär).
/// Wird über `anyhow` transportiert; der Radar erkennt ihn per `downcast_ref` und wartet.
#[derive(Debug, Clone, PartialEq)]
pub struct RateLimited {
    /// Ab hier ist das Kontingent wieder frei (`x-ratelimit-reset` bzw. `rateLimit.resetAt`).
    pub reset_at: Option<DateTime<Utc>>,
    /// Von GitHub verlangte Wartezeit (`Retry-After`).
    pub retry_after: Option<Duration>,
}

impl RateLimited {
    /// Wie lange bis zum nächsten sinnvollen Versuch; ohne Zeitangabe `UNKNOWN_RATE_LIMIT_WAIT`.
    pub fn wait(&self, now: DateTime<Utc>) -> Duration {
        self.retry_at(now).map_or(UNKNOWN_RATE_LIMIT_WAIT, |at| {
            (at - now).to_std().unwrap_or_default()
        })
    }

    /// Zeitpunkt, ab dem ein neuer Versuch sinnvoll ist.
    pub fn retry_at(&self, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
        let after = self
            .retry_after
            .and_then(|d| chrono::Duration::from_std(d).ok())
            .map(|d| now + d);
        match (self.reset_at, after) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        }
    }
}

impl std::fmt::Display for RateLimited {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.retry_at(Utc::now()) {
            Some(at) => f.write_str(&trf!(
                "GitHub API quota used up. Available again at {}.",
                "GitHub-API-Kontingent aufgebraucht. Wieder frei um {} Uhr.",
                crate::time::clock(at)
            )),
            None => f.write_str(tr!(
                "GitHub is throttling requests right now. Retrying in a few minutes.",
                "GitHub drosselt gerade die Anfragen. Neuer Versuch in ein paar Minuten."
            )),
        }
    }
}

impl std::error::Error for RateLimited {}

/// GitHub hat innerhalb der Frist nicht vollständig geantwortet (Verbindung, Header oder Body).
/// Läuft wie jeder andere Fehler in den Backoff des Radars.
#[derive(Debug, Clone, PartialEq)]
pub struct TimedOut(pub Duration);

impl std::fmt::Display for TimedOut {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&trf!(
            "GitHub did not respond within {} s. Retrying automatically.",
            "GitHub hat nicht innerhalb von {} s geantwortet. Neuer Versuch folgt automatisch.",
            self.0.as_secs()
        ))
    }
}

impl std::error::Error for TimedOut {}

/// Vorübergehender Fehler beim Anmelden (Verbindung, abgebrochene Antwort, 408/5xx): Die
/// Anmeldung fragt weiter, statt aufzugeben.
#[derive(Debug)]
pub struct Transient(pub String);

impl std::fmt::Display for Transient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Transient {}

impl Transient {
    /// Lohnt sich ein neuer Versuch nach diesem Fehler?
    pub fn matches(e: &anyhow::Error) -> bool {
        e.is::<Transient>() || e.is::<TimedOut>()
    }
}

/// Bricht `work` nach `limit` mit `TimedOut` ab. Der Timer läuft unabhängig vom Executor
/// (`async-io`), damit auch ein Request abbricht, dessen Client nie wieder aufweckt.
async fn within<T>(limit: Duration, work: impl Future<Output = Result<T>>) -> Result<T> {
    let timeout = async {
        async_io::Timer::after(limit).await;
        Err(TimedOut(limit).into())
    };
    futures::future::select(std::pin::pin!(work), std::pin::pin!(timeout))
        .await
        .factor_first()
        .0
}

/// Drosselung ohne Zeitangabe (z. B. sekundäres Limit ohne `Retry-After`): so lange warten,
/// bei wiederholter Drosselung verdoppelt der Radar die Pause.
pub const UNKNOWN_RATE_LIMIT_WAIT: Duration = Duration::from_secs(120);

/// Drossel-Angaben aus den Antwort-Headern.
struct LimitHeaders {
    /// `x-ratelimit-remaining: 0`
    exhausted: bool,
    reset_at: Option<DateTime<Utc>>,
    retry_after: Option<Duration>,
}

impl LimitHeaders {
    fn read(headers: &gpui_kit::http_client::http::HeaderMap) -> Self {
        let header = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
        Self {
            exhausted: header("x-ratelimit-remaining").is_some_and(|v| v.trim() == "0"),
            reset_at: header("x-ratelimit-reset")
                .and_then(|v| v.trim().parse::<i64>().ok())
                .and_then(|secs| DateTime::from_timestamp(secs, 0)),
            retry_after: header("retry-after")
                .and_then(|v| v.trim().parse::<u64>().ok())
                .map(Duration::from_secs),
        }
    }

    /// Drosselung mit den Zeitangaben aus den Headern; `reset_at` gilt nur bei leerem Kontingent.
    fn limited(&self) -> RateLimited {
        RateLimited {
            reset_at: if self.exhausted { self.reset_at } else { None },
            retry_after: self.retry_after,
        }
    }
}

/// Meldet GitHub im Body ein sekundäres Limit („secondary rate limit“, früher „abuse detection“)?
fn mentions_secondary_limit(body: &str) -> bool {
    let body = body.to_lowercase();
    body.contains("secondary rate limit") || body.contains("abuse detection")
}

/// Liest die Drossel-Angaben aus einer Antwort, falls GitHub drosselt. Ein sekundäres Limit
/// ohne Zeitangabe bleibt ohne Zeit: Der Radar wartet dann `UNKNOWN_RATE_LIMIT_WAIT` und
/// verlängert bei Wiederholung exponentiell, wie GitHub es verlangt.
fn rate_limited(
    status: u16,
    headers: &gpui_kit::http_client::http::HeaderMap,
    body: &str,
) -> Option<RateLimited> {
    let h = LimitHeaders::read(headers);
    let secondary = status == 403 && mentions_secondary_limit(body);
    let throttled =
        status == 429 || (status == 403 && (h.exhausted || h.retry_after.is_some() || secondary));
    throttled.then(|| h.limited())
}

/// Verständlicher Text für einen HTTP-Fehler statt des rohen Antwort-Bodys.
fn describe_http_error(status: u16, body: &str) -> String {
    // GitHub liefert meist `{"message": "...", "documentation_url": "..."}`.
    let detail = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v["message"].as_str().map(str::to_owned))
        .filter(|m| !m.trim().is_empty())
        .map(|m| {
            let m = m.chars().take(160).collect::<String>();
            trf!(" GitHub says: “{m}”", " GitHub meldet: „{m}“")
        })
        .unwrap_or_default();
    let text = match status {
        403 => tr!(
            "GitHub denied access (403). Does the token lack permissions for these repos?",
            "GitHub verweigert den Zugriff (403). Fehlen dem Token Rechte für diese Repos?"
        ),
        404 => tr!(
            "GitHub does not know this address (404).",
            "GitHub kennt die Adresse nicht (404)."
        ),
        408 | 504 => tr!(
            "GitHub did not respond in time. Retrying automatically.",
            "GitHub hat nicht rechtzeitig geantwortet. Neuer Versuch folgt automatisch."
        ),
        500..=599 => tr!(
            "GitHub is having problems right now. Retrying automatically.",
            "GitHub hat gerade Probleme. Neuer Versuch folgt automatisch."
        ),
        _ => tr!(
            "GitHub rejected the request.",
            "GitHub hat die Anfrage abgelehnt."
        ),
    };
    if (500..=599).contains(&status) {
        format!("{text} (HTTP {status})")
    } else {
        format!("{text}{detail}")
    }
}

/// Klartext für fehlende Rechte statt GitHubs Rohmeldung; `None`, wenn es kein Rechtefehler ist.
fn explain_access_error(message: &str, kind: Option<&str>) -> Option<String> {
    explain_oauth_restriction(message).or_else(|| explain_missing_write(message, kind))
}

/// Klartext, wenn eine Org OAuth-Apps einschränkt und PR Radar nicht freigegeben hat. Gilt für
/// Lesen und Schreiben, daher auch beim Abruf der Repos.
fn explain_oauth_restriction(message: &str) -> Option<String> {
    let lower = message.to_lowercase();
    if lower.contains("oauth app access restrictions") {
        // „… the `acme` organization has enabled OAuth App access restrictions …“
        let org = message
            .split('`')
            .nth(1)
            .filter(|o| !o.is_empty() && !o.contains(' '));
        return Some(match org {
            Some(o) => trf!(
                "The organization “{o}” has not approved PR Radar yet. An owner can allow it under \
                 Settings → Third-party access → OAuth app policy.",
                "Die Organisation „{o}“ hat PR Radar noch nicht freigegeben. Ein Owner kann das unter \
                 Settings → Third-party access → OAuth app policy erlauben."
            ),
            None => tr!(
                "The organization has not approved PR Radar yet. An owner can allow it under \
                 Settings → Third-party access → OAuth app policy.",
                "Die Organisation hat PR Radar noch nicht freigegeben. Ein Owner kann das unter \
                 Settings → Third-party access → OAuth app policy erlauben."
            )
            .to_string(),
        });
    }
    None
}

/// Klartext für fehlende Schreibrechte – nur für Mutationen: Beim Lesen bedeutet `FORBIDDEN`
/// etwas anderes (z. B. SAML oder ein Token ohne Zugriff aufs Repo).
fn explain_missing_write(message: &str, kind: Option<&str>) -> Option<String> {
    let lower = message.to_lowercase();
    let denied = kind == Some("FORBIDDEN")
        || [
            "resource not accessible by",
            "does not have the correct permissions",
            "does not have permission",
            "must have push access",
            "must have write access",
            "not authorized",
        ]
        .iter()
        .any(|p| lower.contains(p));
    denied.then(|| {
        tr!(
            "No write access to this pull request. Merging and auto-merge need write access \
             to the repo and, for the token, “Pull requests: write” (fine-grained PAT) or the `repo` scope.",
            "Keine Schreibrechte für diesen Pull Request. Mergen und Auto-Merge brauchen Schreibrechte \
             im Repo und beim Token „Pull requests: write“ (Fine-grained PAT) bzw. den Scope `repo`."
        )
        .to_string()
    })
}

/// HTTP-Status, nach denen ein neuer Versuch sinnvoll ist.
fn is_transient_status(status: u16) -> bool {
    status == 408 || status == 429 || (500..=599).contains(&status)
}

/// Fehler einer Mutation: Rechtefehler im Klartext, sonst GitHubs Meldungen.
fn describe_mutation_errors(errors: &[GqlError]) -> String {
    errors
        .iter()
        .find_map(|e| explain_access_error(&e.message, e.kind.as_deref()))
        .unwrap_or_else(|| {
            errors
                .iter()
                .map(|e| e.message.as_str())
                .collect::<Vec<_>>()
                .join("; ")
        })
}

/// Ergebnis eines Abrufs aller Repos.
pub struct Fetched {
    pub snapshot: Snapshot,
    pub rate_limit: Option<RateLimit>,
}

pub struct GitHub {
    http: Arc<dyn HttpClient>,
    token: Mutex<Option<Token>>,
    /// Von GitHub abgelehnte Anmeldung, die sich nicht aus dem Schlüsselbund löschen ließ: wird
    /// bis zur nächsten Anmeldung nicht wieder eingelesen.
    stale: Mutex<Option<String>>,
    rules: Mutex<RulesCache>,
    /// Zuletzt gesehene Zahl offener PRs je Repo – bestimmt die Seitengröße der nächsten Abfrage.
    open_counts: Mutex<HashMap<String, usize>>,
    /// Frist je Request samt Body (`REQUEST_TIMEOUT`, in Tests kürzer).
    timeout: Duration,
}

/// Wie lange Branch-Regeln gültig bleiben – sie ändern sich selten, kosten aber REST-Kontingent.
const RULES_TTL: Duration = Duration::from_secs(600);

/// Branch-Regeln je (Repo, Branch). Bei Abruffehlern gilt der letzte bekannte Stand weiter,
/// damit nicht still wieder alle Merge-Methoden angeboten werden.
#[derive(Default)]
struct RulesCache {
    entries: HashMap<(String, String), RulesEntry>,
}

struct RulesEntry {
    at: Instant,
    rules: Value,
    /// `ETag` der letzten Antwort: Eine Nachfrage mit `If-None-Match` beantwortet GitHub mit 304,
    /// und die zählt nicht gegen das Kontingent.
    etag: Option<String>,
}

/// Ergebnis eines Abrufs der Branch-Regeln.
#[derive(Debug, PartialEq)]
enum RulesFetch {
    Fresh {
        rules: Value,
        etag: Option<String>,
    },
    /// 304: Der zwischengespeicherte Stand gilt weiter.
    NotModified,
    Failed,
}

impl RulesCache {
    fn fresh(&self, key: &(String, String), now: Instant) -> Option<Value> {
        self.entries
            .get(key)
            .filter(|e| now.duration_since(e.at) < RULES_TTL)
            .map(|e| e.rules.clone())
    }

    fn etag(&self, key: &(String, String)) -> Option<String> {
        self.entries.get(key)?.etag.clone()
    }

    /// Übernimmt ein Abrufergebnis; bei 304 gilt der alte Eintrag wieder `RULES_TTL` lang, bei
    /// einem Fehler bleibt er gültig, wird aber beim nächsten Mal erneut angefragt.
    fn update(
        &mut self,
        key: (String, String),
        now: Instant,
        fetched: RulesFetch,
    ) -> Option<Value> {
        match fetched {
            RulesFetch::Fresh { rules, etag } => {
                self.entries.insert(
                    key,
                    RulesEntry {
                        at: now,
                        rules: rules.clone(),
                        etag,
                    },
                );
                Some(rules)
            }
            RulesFetch::NotModified => {
                let e = self.entries.get_mut(&key)?;
                e.at = now;
                Some(e.rules.clone())
            }
            RulesFetch::Failed => self.entries.get(&key).map(|e| e.rules.clone()),
        }
    }
}

impl GitHub {
    pub fn new(http: Arc<dyn HttpClient>) -> Self {
        Self {
            http,
            token: Mutex::new(None),
            stale: Mutex::new(None),
            rules: Mutex::new(RulesCache::default()),
            open_counts: Mutex::new(HashMap::new()),
            timeout: REQUEST_TIMEOUT,
        }
    }

    pub fn token_source(&self) -> Option<TokenSource> {
        self.token.lock().ok()?.as_ref().map(|t| t.source)
    }

    /// Token aus der gespeicherten Anmeldung, `GITHUB_TOKEN`/`GH_TOKEN` oder der GitHub-CLI.
    /// Blockiert – nur im Hintergrund aufrufen.
    fn token(&self) -> Result<Token> {
        if let Some(t) = self
            .token
            .lock()
            .map_err(|_| anyhow!(tr!("Token cache locked", "Token-Cache gesperrt")))?
            .clone()
        {
            return Ok(t);
        }
        let stale = self.stale.lock().ok().and_then(|s| s.clone());
        let token = find_token(stale.as_deref()).ok_or_else(|| {
            anyhow!(tr!(
                "Not signed in to GitHub. Sign in with GitHub under Repositories (⌘,), \
                 or set GITHUB_TOKEN or use `gh auth login`.",
                "Nicht bei GitHub angemeldet. Melde dich unter Repositories (⌘,) mit GitHub an, \
                 oder setze GITHUB_TOKEN bzw. nutze `gh auth login`."
            ))
        })?;
        *self
            .token
            .lock()
            .map_err(|_| anyhow!(tr!("Token cache locked", "Token-Cache gesperrt")))? =
            Some(token.clone());
        Ok(token)
    }

    /// Vergisst den Token nach einer 401 – aber nur, wenn noch der abgelehnte gilt. Eine Anmeldung,
    /// die während der Anfrage fertig wurde, bleibt so erhalten. Eine abgelehnte Anmeldung
    /// verschwindet auch aus dem Schlüsselbund, damit der nächste Start sie nicht wieder probiert.
    /// Vergleich, Vergessen und Löschen laufen unter derselben Sperre wie `sign_in`.
    /// Lässt sie sich dort nicht löschen, wird sie bis zur nächsten Anmeldung übergangen und der
    /// Fehler zurückgegeben.
    fn forget_rejected(&self, rejected: &str) -> Option<anyhow::Error> {
        let mut current = self.token.lock().ok()?;
        if current.as_ref().is_none_or(|t| t.value != rejected) {
            return None;
        }
        if current.take().map(|t| t.source) != Some(TokenSource::OAuth) {
            return None;
        }
        let e = crate::keychain::delete().err()?;
        log::warn!("{e:#}");
        if let Ok(mut stale) = self.stale.lock() {
            *stale = Some(rejected.to_string());
        }
        Some(e)
    }

    /// Übernimmt den Token aus der GitHub-Anmeldung; er gilt vor `GITHUB_TOKEN` und `gh` und
    /// bleibt im Schlüsselbund, bis man sich abmeldet oder GitHub ihn ablehnt. Scheitert das
    /// Speichern, gilt die Anmeldung trotzdem (bis zum Beenden) und der Fehler kommt zurück.
    pub fn sign_in(&self, token: String) -> Result<()> {
        let saved = match self.token.lock() {
            Ok(mut t) => {
                let saved = crate::keychain::save(&token);
                *t = Some(Token {
                    value: token,
                    source: TokenSource::OAuth,
                });
                saved
            }
            Err(_) => Err(anyhow!(tr!("Token cache locked", "Token-Cache gesperrt"))),
        };
        if let Ok(mut stale) = self.stale.lock() {
            *stale = None;
        }
        // Branch-Regeln hängen an den Rechten des Tokens.
        if let Ok(mut rules) = self.rules.lock() {
            *rules = RulesCache::default();
        }
        saved
    }

    /// „Abmelden“: Token aus Speicher und Schlüsselbund entfernen. Danach gelten wieder
    /// `GITHUB_TOKEN` bzw. die GitHub-CLI, falls vorhanden.
    /// Scheitert das Löschen im Schlüsselbund, bleibt alles, wie es war, und der Fehler geht an
    /// die Oberfläche: Sonst wäre man nach dem nächsten Start still wieder angemeldet.
    pub fn sign_out(&self) -> Result<()> {
        let mut token = self
            .token
            .lock()
            .map_err(|_| anyhow!(tr!("Token cache locked", "Token-Cache gesperrt")))?;
        crate::keychain::delete()?;
        *token = None;
        drop(token);
        if let Ok(mut rules) = self.rules.lock() {
            *rules = RulesCache::default();
        }
        Ok(())
    }

    /// Device Flow, Schritt 1: Code für die Anmeldung anfordern.
    pub async fn request_device_code(&self, client_id: &str) -> Result<oauth::DeviceCode> {
        let body = self
            .post_form(oauth::DEVICE_CODE_URL, oauth::device_code_form(client_id))
            .await?;
        oauth::parse_device_code(&body)
    }

    /// Device Flow, Schritt 2: Fragen, ob der Nutzer den Code bestätigt hat.
    pub async fn poll_device_token(
        &self,
        client_id: &str,
        device_code: &str,
        interval: Duration,
    ) -> Result<oauth::PollStep> {
        let body = self
            .post_form(oauth::TOKEN_URL, oauth::token_form(client_id, device_code))
            .await?;
        oauth::parse_token_response(&body, interval)
    }

    /// POST mit Formular-Body an github.com (nicht die API); liefert den JSON-Body.
    async fn post_form(&self, url: &str, form: String) -> Result<String> {
        let req = Request::builder()
            .method(Method::POST)
            .uri(url)
            .header("Accept", "application/json")
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header("User-Agent", "pr-radar-native")
            .body(AsyncBody::from(form.into_bytes()))?;
        let (status, text) = within(self.timeout, async {
            let mut res = self.http.send(req).await.map_err(|e| {
                Transient(trf!(
                    "GitHub unreachable: {e:#}",
                    "GitHub nicht erreichbar: {e:#}"
                ))
            })?;
            let status = res.status();
            let mut text = String::new();
            res.body_mut()
                .read_to_string(&mut text)
                .await
                .map_err(|e| {
                    Transient(trf!(
                        "Response from GitHub interrupted: {e}",
                        "Antwort von GitHub abgebrochen: {e}"
                    ))
                })?;
            Ok((status, text))
        })
        .await?;
        let status = status.as_u16();
        // 408/429/5xx sind vorübergehend, auch wenn der Body ein `error` enthält.
        if is_transient_status(status) {
            return Err(Transient(describe_http_error(status, &text)).into());
        }
        // Fehler des Device Flows kommen als JSON mit `error`, auch bei HTTP 4xx.
        if !(200..300).contains(&status) && !text.contains("\"error\"") {
            bail!("{}", describe_http_error(status, &text));
        }
        Ok(text)
    }

    async fn request(&self, query: &str, variables: Value) -> Result<GqlResponse> {
        let token = self.token()?;
        let body = serde_json::to_vec(&json!({ "query": query, "variables": variables }))?;
        let req = Request::builder()
            .method(Method::POST)
            .uri(ENDPOINT)
            .header("Authorization", format!("bearer {}", token.value))
            .header("Content-Type", "application/json")
            .header("User-Agent", "pr-radar-native")
            .body(AsyncBody::from(body))?;
        // Senden und Body-Lesen teilen sich eine Frist: Hängt GitHub an irgendeiner Stelle,
        // meldet der Poll-Zyklus einen Fehler und wartet mit Backoff, statt still zu stehen.
        let (status, headers, text) = within(self.timeout, async {
            let mut res = self
                .http
                .send(req)
                .await
                .context(tr!("GitHub is unreachable", "GitHub nicht erreichbar"))?;
            let status = res.status();
            let headers = res.headers().clone();
            // Was schon Status und Header als Drosselung zeigen, geht ohne Body an den Poller:
            // Ein hängender oder nicht lesbarer Body darf die Wartezeit nicht verschlucken.
            if let Some(limited) = rate_limited(status.as_u16(), &headers, "") {
                return Err(limited.into());
            }
            let mut text = String::new();
            res.body_mut().read_to_string(&mut text).await.context(tr!(
                "Response from GitHub was not read completely",
                "Antwort von GitHub nicht vollständig gelesen"
            ))?;
            Ok((status, headers, text))
        })
        .await?;
        // Sekundäres Limit ohne Header-Hinweis erkennt man nur an GitHubs Meldung im Body.
        if let Some(limited) = rate_limited(status.as_u16(), &headers, &text) {
            return Err(limited.into());
        }
        if status.as_u16() == 401 {
            let not_deleted = self.forget_rejected(&token.value);
            if token.source == TokenSource::OAuth {
                let msg = tr!(
                    "GitHub no longer accepts the sign-in (401). Please sign in again.",
                    "GitHub akzeptiert die Anmeldung nicht mehr (401). Bitte erneut anmelden."
                );
                match not_deleted {
                    Some(e) => bail!("{msg} ({e:#})"),
                    None => bail!("{msg}"),
                }
            }
            bail!(tr!(
                "GitHub rejected the token (401). Check the token.",
                "GitHub lehnt den Token ab (401). Token prüfen."
            ));
        }
        if !status.is_success() {
            bail!("{}", describe_http_error(status.as_u16(), &text));
        }
        let res: GqlResponse = serde_json::from_str(&text).context(tr!(
            "Unexpected response from GitHub",
            "Unerwartete Antwort von GitHub"
        ))?;
        // GraphQL meldet das erschöpfte Kontingent mit HTTP 200 und Fehlertyp RATE_LIMITED;
        // die Reset-Zeit steht dann trotzdem in den Headern.
        if res
            .errors
            .iter()
            .any(|e| e.kind.as_deref() == Some("RATE_LIMITED"))
        {
            return Err(LimitHeaders::read(&headers).limited().into());
        }
        Ok(res)
    }

    /// Führt eine Mutation aus; GraphQL-Fehler werden zu einem `Err`.
    async fn mutate(&self, query: &str, variables: Value) -> Result<()> {
        // Aktionen wiederholt niemand automatisch, und GitHub kann sie trotz Zeitüberschreitung
        // ausgeführt haben: Die Meldung darf weder Erfolg noch einen neuen Versuch versprechen.
        let res = self.request(query, variables).await.map_err(|e| {
            match e.downcast_ref::<TimedOut>() {
                Some(TimedOut(limit)) => anyhow!(trf!(
                    "GitHub did not respond within {} s. The next refresh shows whether the action went through.",
                    "GitHub hat nicht innerhalb von {} s geantwortet. Ob die Aktion ausgeführt wurde, zeigt die nächste Aktualisierung.",
                    limit.as_secs()
                )),
                None => e,
            }
        })?;
        if !res.errors.is_empty() {
            bail!("{}", describe_mutation_errors(&res.errors));
        }
        Ok(())
    }

    /// Auto-Merge an/aus. `head`: nur, wenn der Branch noch auf diesem Commit steht.
    pub async fn set_auto_merge(
        &self,
        pr_id: &str,
        method: Option<MergeMethod>,
        head: Option<&str>,
    ) -> Result<()> {
        match method {
            Some(m) => {
                self.mutate(
                    queries::ENABLE_AUTO_MERGE,
                    json!({ "id": pr_id, "method": m.graphql(), "head": head }),
                )
                .await
            }
            None => {
                self.mutate(queries::DISABLE_AUTO_MERGE, json!({ "id": pr_id }))
                    .await
            }
        }
    }

    /// Mergt – nur, wenn der Branch noch auf `head` steht (sonst lehnt GitHub ab).
    pub async fn merge(&self, pr_id: &str, method: MergeMethod, head: &str) -> Result<()> {
        self.mutate(
            queries::MERGE_PR,
            json!({ "id": pr_id, "method": method.graphql(), "head": head }),
        )
        .await
    }

    /// Lädt alle Repos (in Blöcken zu je `CHUNK`) und baut daraus einen Snapshot.
    pub async fn fetch(&self, repos: &[String]) -> Result<Fetched> {
        let sized = self.page_sizes(repos);
        let mut round = self.fetch_round(&sized).await?;
        // Mehr offene PRs als die verkleinerte Seite fasst: genau diese Repos sofort mit
        // passender Seite nachladen, damit kein PR auch nur für einen Poll fehlt.
        let truncated: Vec<(String, usize)> = round
            .repos
            .iter()
            .zip(&sized)
            .filter_map(|((full, raw), (_, page))| {
                let total = raw.as_ref().ok()?.open.total_count?;
                (total > *page && *page < queries::MAX_OPEN_PAGE)
                    .then(|| (full.clone(), queries::open_page_size(Some(total))))
            })
            .collect();
        if !truncated.is_empty() {
            let again = self.fetch_round(&truncated).await?;
            round.rate_limit = merge_rate_limits(round.rate_limit, again.rate_limit);
            for (full, raw) in again.repos {
                if let Some(slot) = round.repos.iter_mut().find(|(f, _)| *f == full) {
                    slot.1 = raw;
                }
            }
        }
        self.remember_open_counts(&round.repos);

        let mut snapshot = Snapshot {
            viewer: round.viewer,
            ..Snapshot::default()
        };
        for (full, raw) in &round.repos {
            match raw {
                Ok(raw) => normalize::push_repo(&mut snapshot, raw),
                Err(msg) => snapshot
                    .repos
                    .push(RepoInfo::unavailable(full, msg.clone())),
            }
        }
        let rate_limit = round.rate_limit;

        self.apply_rulesets(&mut snapshot).await;
        snapshot
            .open
            .sort_by_key(|p| std::cmp::Reverse(p.updated_at));
        snapshot
            .merged
            .sort_by_key(|m| std::cmp::Reverse(m.merged_at));
        snapshot
            .releases
            .sort_by_key(|r| std::cmp::Reverse(r.published_at));
        Ok(Fetched {
            snapshot,
            rate_limit,
        })
    }

    /// Setzt je PR die effektiv erlaubten Merge-Methoden (Repo-Einstellungen ∩ Rulesets des Ziel-Branches).
    async fn apply_rulesets(&self, snapshot: &mut Snapshot) {
        let mut targets: Vec<(String, String)> = Vec::new();
        for pr in &snapshot.open {
            let key = (pr.repo.clone(), pr.base_ref.clone());
            if !targets.contains(&key) {
                targets.push(key);
            }
        }
        let rules = futures::future::join_all(
            targets
                .iter()
                .map(|(repo, branch)| self.branch_rules(repo, branch)),
        )
        .await;
        for pr in &mut snapshot.open {
            let repo_methods = snapshot
                .repos
                .iter()
                .find(|r| r.full_name == pr.repo)
                .map(|r| r.merge_methods.clone())
                .unwrap_or_default();
            let i = targets
                .iter()
                .position(|(r, b)| *r == pr.repo && *b == pr.base_ref);
            pr.merge_methods = match i.and_then(|i| rules[i].as_ref()) {
                Some(rules) => normalize::apply_branch_rules(&repo_methods, rules),
                // Regeln nicht lesbar: GitHub lehnt Unerlaubtes beim Mergen ohnehin ab.
                None => repo_methods,
            };
        }
    }

    /// Effektive Regeln eines Branches (inkl. Org-Rulesets); `None`, wenn nicht abrufbar.
    async fn branch_rules(&self, repo: &str, branch: &str) -> Option<Value> {
        let key = (repo.to_string(), branch.to_string());
        let etag = {
            let cache = self.rules.lock().ok()?;
            if let Some(v) = cache.fresh(&key, Instant::now()) {
                return Some(v);
            }
            cache.etag(&key)
        };
        let fetched = self.fetch_branch_rules(repo, branch, etag.as_deref()).await;
        self.rules.lock().ok()?.update(key, Instant::now(), fetched)
    }

    async fn fetch_branch_rules(&self, repo: &str, branch: &str, etag: Option<&str>) -> RulesFetch {
        let Ok(token) = self.token() else {
            return RulesFetch::Failed;
        };
        let mut req = Request::builder()
            .uri(format!(
                "https://api.github.com/repos/{repo}/rules/branches/{}",
                encode_path(branch)
            ))
            .header("Authorization", format!("bearer {}", token.value))
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "pr-radar-native");
        if let Some(etag) = etag {
            req = req.header("If-None-Match", etag);
        }
        let Ok(req) = req.body(AsyncBody::empty()) else {
            return RulesFetch::Failed;
        };
        // Bei Zeitüberschreitung gilt wie bei jedem Abruffehler der letzte bekannte Stand.
        let res = within(self.timeout, async {
            let mut res = self.http.send(req).await?;
            if res.status().as_u16() == 304 {
                return Ok(None);
            }
            if !res.status().is_success() {
                bail!("HTTP {}", res.status());
            }
            let etag = res
                .headers()
                .get("etag")
                .and_then(|v| v.to_str().ok())
                .map(String::from);
            let mut text = String::new();
            res.body_mut().read_to_string(&mut text).await?;
            Ok(Some((text, etag)))
        })
        .await;
        match res {
            Ok(None) => RulesFetch::NotModified,
            Ok(Some((text, etag))) => match serde_json::from_str(&text) {
                Ok(rules) => RulesFetch::Fresh { rules, etag },
                Err(_) => RulesFetch::Failed,
            },
            Err(_) => RulesFetch::Failed,
        }
    }

    async fn request_dashboard(&self, chunk: &[(String, usize)]) -> Result<GqlResponse> {
        self.request(&queries::dashboard_query(chunk), json!({}))
            .await
    }

    /// Seitengröße für die offenen PRs je Repo, nach der zuletzt gesehenen Anzahl.
    fn page_sizes(&self, repos: &[String]) -> Vec<(String, usize)> {
        let counts = self.open_counts.lock().ok();
        repos
            .iter()
            .map(|r| {
                let known = counts.as_ref().and_then(|c| c.get(r).copied());
                (r.clone(), queries::open_page_size(known))
            })
            .collect()
    }

    fn remember_open_counts(&self, repos: &[(String, Result<RawRepo, String>)]) {
        let Ok(mut counts) = self.open_counts.lock() else {
            return;
        };
        for (full, raw) in repos {
            match raw.as_ref().ok().and_then(|r| r.open.total_count) {
                Some(n) => {
                    counts.insert(full.clone(), n);
                }
                // Unbekannt (Fehler, kein Zugriff): beim nächsten Mal wieder die volle Seite.
                None => {
                    counts.remove(full);
                }
            }
        }
    }

    /// Ein Durchgang über alle übergebenen Repos (in Blöcken zu je `CHUNK`, parallel).
    async fn fetch_round(&self, sized: &[(String, usize)]) -> Result<Round> {
        let chunks: Vec<&[(String, usize)]> = sized.chunks(CHUNK).collect();
        let results =
            futures::future::join_all(chunks.iter().map(|c| self.request_dashboard(c))).await;
        // Ein Rate-Limit in irgendeinem Chunk zählt vor anderen Fehlern, damit der
        // Backoff bis zum Reset wartet statt zu früh erneut anzufragen.
        if let Some(limited) = longest_rate_limit(&results, Utc::now()) {
            return Err(limited.into());
        }

        let mut round = Round::default();
        for (chunk, res) in chunks.into_iter().zip(results) {
            let res = res?;
            let Some(data) = res.data else {
                let msg = res
                    .errors
                    .iter()
                    .map(|e| e.message.as_str())
                    .collect::<Vec<_>>()
                    .join("; ");
                bail!(
                    "{}",
                    if msg.is_empty() {
                        tr!("Empty response from GitHub", "Leere Antwort von GitHub").into()
                    } else {
                        msg
                    }
                );
            };
            if round.viewer.is_none() {
                round.viewer = data
                    .get("viewer")
                    .and_then(|v| v["login"].as_str())
                    .map(String::from);
            }
            if let Some(rl) = data.get("rateLimit") {
                let rl = RateLimit {
                    remaining: rl["remaining"].as_u64().unwrap_or(0),
                    limit: rl["limit"].as_u64().unwrap_or(0),
                    reset_at: rl["resetAt"].as_str().and_then(|s| s.parse().ok()),
                    cost: rl["cost"].as_u64().unwrap_or(1),
                };
                round.rate_limit = merge_rate_limits(round.rate_limit, Some(rl));
            }
            for (i, (full, _)) in chunk.iter().enumerate() {
                let alias = format!("r{i}");
                let raw = data.get(&alias).filter(|v| !v.is_null()).cloned();
                let entry = match raw.map(serde_json::from_value::<RawRepo>) {
                    Some(Ok(raw)) => Ok(raw),
                    Some(Err(e)) => Err(trf!(
                        "Unreadable response: {e}",
                        "Antwort nicht lesbar: {e}"
                    )),
                    None => {
                        let err = res.errors.iter().find(|e| {
                            e.path.first().and_then(Value::as_str) == Some(alias.as_str())
                        });
                        Err(err
                            .map(|e| {
                                explain_oauth_restriction(&e.message)
                                    .unwrap_or_else(|| e.message.clone())
                            })
                            .unwrap_or_else(|| {
                                tr!(
                                    "Repository not found or no access",
                                    "Repository nicht gefunden oder kein Zugriff"
                                )
                                .into()
                            }))
                    }
                };
                round.repos.push((full.clone(), entry));
            }
        }
        Ok(round)
    }
}

/// Rohdaten eines Abrufdurchgangs, je Repo in Abfragereihenfolge.
#[derive(Default)]
struct Round {
    viewer: Option<String>,
    repos: Vec<(String, std::result::Result<RawRepo, String>)>,
    rate_limit: Option<RateLimit>,
}

/// Fasst die Kontingent-Angaben mehrerer Requests eines Polls zusammen: Kosten addieren sich,
/// beim Rest zählt die knappste (jüngste) Angabe.
fn merge_rate_limits(a: Option<RateLimit>, b: Option<RateLimit>) -> Option<RateLimit> {
    match (a, b) {
        (Some(a), Some(b)) => {
            let cost = a.cost + b.cost;
            let mut lower = if b.remaining <= a.remaining { b } else { a };
            lower.cost = cost;
            Some(lower)
        }
        (a, b) => a.or(b),
    }
}

/// Unter allen gedrosselten Chunks der mit der längsten Wartezeit.
fn longest_rate_limit<T>(results: &[Result<T>], now: DateTime<Utc>) -> Option<RateLimited> {
    results
        .iter()
        .filter_map(|r| r.as_ref().err()?.downcast_ref::<RateLimited>())
        .max_by_key(|l| l.wait(now))
        .cloned()
}

/// Gespeicherte Anmeldung (außer der abgelehnten `stale`), sonst `GITHUB_TOKEN`/`GH_TOKEN`,
/// sonst die GitHub-CLI.
fn find_token(stale: Option<&str>) -> Option<Token> {
    if let Some(value) = usable_saved(crate::keychain::load(), stale) {
        return Some(Token {
            value,
            source: TokenSource::OAuth,
        });
    }
    for key in ["GITHUB_TOKEN", "GH_TOKEN"] {
        if let Ok(v) = std::env::var(key) {
            let v = v.trim().to_string();
            if !v.is_empty() {
                return Some(Token {
                    value: v,
                    source: TokenSource::Env,
                });
            }
        }
    }
    // Aus dem Finder gestartet fehlt der Shell-PATH – daher auch feste Pfade probieren.
    let candidates = [
        "gh",
        "/opt/homebrew/bin/gh",
        "/usr/local/bin/gh",
        "/usr/bin/gh",
    ];
    candidates.iter().find_map(|bin| {
        let out = Command::new(bin).args(["auth", "token"]).output().ok()?;
        let value = String::from_utf8(out.stdout).ok()?.trim().to_string();
        (out.status.success() && !value.is_empty()).then_some(Token {
            value,
            source: TokenSource::GhCli,
        })
    })
}

/// Gespeicherte Anmeldung, sofern GitHub sie nicht schon abgelehnt hat.
fn usable_saved(saved: Option<String>, stale: Option<&str>) -> Option<String> {
    saved.filter(|v| Some(v.as_str()) != stale)
}

/// Prozent-Kodierung für ein Pfadsegment (Branch-Namen dürfen `/` enthalten).
fn encode_path(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::future::BoxFuture;
    use gpui_kit::http_client::{Response, Url, http::HeaderValue};
    use std::pin::Pin;
    use std::task::{Context, Poll};

    /// Fake-Client, der hängt: entweder schon beim Senden oder beim Lesen des Bodys.
    struct HangingClient {
        hang_in_body: bool,
    }

    /// Body, der nie Daten liefert und nie endet.
    struct NeverBody;

    impl futures::AsyncRead for NeverBody {
        fn poll_read(
            self: Pin<&mut Self>,
            _: &mut Context<'_>,
            _: &mut [u8],
        ) -> Poll<std::io::Result<usize>> {
            Poll::Pending
        }
    }

    impl HttpClient for HangingClient {
        fn user_agent(&self) -> Option<&HeaderValue> {
            None
        }

        fn proxy(&self) -> Option<&Url> {
            None
        }

        fn send(&self, _: Request<AsyncBody>) -> BoxFuture<'static, Result<Response<AsyncBody>>> {
            if self.hang_in_body {
                Box::pin(async {
                    Ok(Response::builder()
                        .status(200)
                        .body(AsyncBody::from_reader(NeverBody))?)
                })
            } else {
                Box::pin(futures::future::pending())
            }
        }
    }

    fn hanging_github(hang_in_body: bool) -> GitHub {
        let mut gh = GitHub::new(Arc::new(HangingClient { hang_in_body }));
        gh.timeout = Duration::from_millis(50);
        *gh.token.lock().unwrap() = Some(Token {
            value: "test".into(),
            source: TokenSource::Env,
        });
        gh
    }

    /// Fake-Client mit fester Antwort (Status und Body).
    struct FixedClient {
        status: u16,
        body: &'static str,
    }

    impl HttpClient for FixedClient {
        fn user_agent(&self) -> Option<&HeaderValue> {
            None
        }

        fn proxy(&self) -> Option<&Url> {
            None
        }

        fn send(&self, _: Request<AsyncBody>) -> BoxFuture<'static, Result<Response<AsyncBody>>> {
            let (status, body) = (self.status, self.body);
            Box::pin(async move {
                Ok(Response::builder()
                    .status(status)
                    .body(AsyncBody::from(body.as_bytes().to_vec()))?)
            })
        }
    }

    fn poll_with(status: u16, body: &'static str) -> Result<oauth::PollStep> {
        let gh = GitHub::new(Arc::new(FixedClient { status, body }));
        futures::executor::block_on(gh.poll_device_token("id", "dc", Duration::from_secs(5)))
    }

    #[test]
    fn serverfehler_beim_anmelden_sind_voruebergehend_auch_mit_error_body() {
        let err = poll_with(503, r#"{"error":"server_error"}"#).unwrap_err();
        assert!(Transient::matches(&err), "{err:#}");
        let err = poll_with(502, "<html>Bad Gateway</html>").unwrap_err();
        assert!(Transient::matches(&err), "{err:#}");
        // Fehler des Device Flows bei 4xx bleiben endgültig
        let err = poll_with(400, r#"{"error":"access_denied"}"#).unwrap_err();
        assert!(!Transient::matches(&err), "{err:#}");
        assert!(err.to_string().contains("declined"), "{err:#}");
        assert_eq!(
            poll_with(200, r#"{"error":"authorization_pending"}"#).unwrap(),
            oauth::PollStep::Pending
        );
    }

    #[test]
    fn haengender_request_bricht_nach_der_frist_ab() {
        for hang_in_body in [false, true] {
            let gh = hanging_github(hang_in_body);
            let started = Instant::now();
            let err = match futures::executor::block_on(gh.fetch(&["a/b".into()])) {
                Ok(_) => panic!("hängender Request darf nicht gelingen"),
                Err(e) => e,
            };
            assert!(started.elapsed() < Duration::from_secs(5));
            assert_eq!(
                err.downcast_ref::<TimedOut>(),
                Some(&TimedOut(Duration::from_millis(50))),
                "{err:#}"
            );
            assert!(
                format!("{err:#}").contains("did not respond within"),
                "{err:#}"
            );
        }
    }

    #[test]
    fn haengende_aktion_verspricht_keinen_neuen_versuch() {
        let gh = hanging_github(false);
        let err = futures::executor::block_on(gh.merge("PR_1", MergeMethod::Squash, "abc"))
            .expect_err("hängende Aktion darf nicht gelingen");
        let msg = err.to_string();
        assert!(msg.contains("did not respond within"), "{msg}");
        assert!(msg.contains("next refresh"), "{msg}");
        assert!(!msg.contains("Retrying"), "{msg}");
    }

    #[test]
    fn haengende_branch_regeln_gelten_als_nicht_abrufbar() {
        // `None` heißt: Repo-Einstellungen gelten, ein älterer Cache-Stand bliebe erhalten
        // (siehe `regeln_bleiben_bei_abruffehler_erhalten`).
        for hang_in_body in [false, true] {
            let gh = hanging_github(hang_in_body);
            let got = futures::executor::block_on(gh.fetch_branch_rules("a/b", "main", None));
            assert_eq!(got, RulesFetch::Failed);
        }
    }

    #[test]
    fn frist_gilt_nur_fuer_haengende_arbeit() {
        let fast = futures::executor::block_on(within(Duration::from_secs(5), async { Ok(7) }));
        assert_eq!(fast.unwrap(), 7);
        let err = futures::executor::block_on(within(Duration::from_millis(10), async {
            futures::future::pending::<Result<()>>().await
        }))
        .unwrap_err();
        assert!(err.downcast_ref::<TimedOut>().is_some());
    }

    fn headers(pairs: &[(&'static str, &str)]) -> gpui_kit::http_client::http::HeaderMap {
        let mut map = gpui_kit::http_client::http::HeaderMap::new();
        for (k, v) in pairs {
            map.insert(*k, v.parse().unwrap());
        }
        map
    }

    #[test]
    fn erkennt_erschoepftes_kontingent() {
        let h = headers(&[
            ("x-ratelimit-remaining", "0"),
            ("x-ratelimit-reset", "1793988000"),
        ]);
        let limited = rate_limited(403, &h, "").expect("gedrosselt");
        assert_eq!(limited.reset_at, DateTime::from_timestamp(1_793_988_000, 0));
        assert_eq!(limited.retry_after, None);
    }

    #[test]
    fn erkennt_sekundaeres_limit_mit_retry_after() {
        let limited = rate_limited(403, &headers(&[("retry-after", "60")]), "").unwrap();
        assert_eq!(limited.retry_after, Some(Duration::from_secs(60)));
        assert!(rate_limited(429, &headers(&[]), "").is_some());
    }

    #[test]
    fn normales_403_ist_keine_drosselung() {
        let h = headers(&[("x-ratelimit-remaining", "4000")]);
        assert_eq!(rate_limited(403, &h, ""), None);
        let denied = r#"{"message":"Resource not accessible by integration"}"#;
        assert_eq!(rate_limited(403, &h, denied), None);
        assert_eq!(rate_limited(502, &headers(&[]), ""), None);
    }

    #[test]
    fn sekundaeres_limit_ohne_retry_after_ist_drosselung_ohne_zeit() {
        let h = headers(&[("x-ratelimit-remaining", "4000")]);
        let body = r#"{"message":"You have exceeded a secondary rate limit. Please wait a few minutes before you try again."}"#;
        let limited = rate_limited(403, &h, body).expect("gedrosselt");
        assert_eq!(limited.reset_at, None);
        assert_eq!(limited.retry_after, None);
        // Ohne Zeitangabe mindestens eine Minute (GitHub-Vorgabe)
        let now = DateTime::from_timestamp(1_000, 0).unwrap();
        assert!(limited.wait(now) >= Duration::from_secs(60));
        // Ältere Formulierung
        let body = r#"{"message":"You have triggered an abuse detection mechanism."}"#;
        assert!(rate_limited(403, &h, body).is_some());
        // Ein mitgeliefertes Retry-After hat Vorrang
        let h = headers(&[("retry-after", "120")]);
        let limited = rate_limited(403, &h, body).unwrap();
        assert_eq!(limited.retry_after, Some(Duration::from_secs(120)));
    }

    #[test]
    fn graphql_drosselung_uebernimmt_reset_header() {
        let h = headers(&[
            ("x-ratelimit-remaining", "0"),
            ("x-ratelimit-reset", "1793988000"),
        ]);
        let limited = LimitHeaders::read(&h).limited();
        assert_eq!(limited.reset_at, DateTime::from_timestamp(1_793_988_000, 0));
        // Reset ohne leeres Kontingent ist kein Wartegrund
        let h = headers(&[
            ("x-ratelimit-remaining", "12"),
            ("x-ratelimit-reset", "1793988000"),
        ]);
        assert_eq!(LimitHeaders::read(&h).limited().reset_at, None);
        assert_eq!(
            LimitHeaders::read(&headers(&[])).limited(),
            RateLimited {
                reset_at: None,
                retry_after: None,
            }
        );
    }

    #[test]
    fn meldet_den_chunk_mit_der_laengsten_wartezeit() {
        let now = DateTime::from_timestamp(1_000, 0).unwrap();
        let limit = |reset: Option<i64>, retry: Option<u64>| -> Result<()> {
            Err(RateLimited {
                reset_at: reset.and_then(|s| DateTime::from_timestamp(s, 0)),
                retry_after: retry.map(Duration::from_secs),
            }
            .into())
        };
        let results = vec![
            Ok(()),
            limit(None, Some(60)),
            Err(anyhow!("anderer Fehler")),
            limit(Some(1_000 + 900), None),
            limit(None, None),
        ];
        let longest = longest_rate_limit(&results, now).expect("gedrosselt");
        assert_eq!(longest.reset_at, DateTime::from_timestamp(1_900, 0));
        // Ohne Zeitangabe zählt die Standardwartezeit (120 s) – länger als 60 s Retry-After
        let results = vec![limit(None, Some(60)), limit(None, None)];
        let longest = longest_rate_limit(&results, now).unwrap();
        assert_eq!(longest.retry_after, None);
        // Kein Rate-Limit dabei
        let results: Vec<Result<()>> = vec![Ok(()), Err(anyhow!("x"))];
        assert_eq!(longest_rate_limit(&results, now), None);
    }

    #[test]
    fn retry_at_nimmt_den_spaeteren_zeitpunkt() {
        let now = DateTime::from_timestamp(1_000, 0).unwrap();
        let limited = RateLimited {
            reset_at: DateTime::from_timestamp(1_100, 0),
            retry_after: Some(Duration::from_secs(300)),
        };
        assert_eq!(limited.retry_at(now), DateTime::from_timestamp(1_300, 0));
        let none = RateLimited {
            reset_at: None,
            retry_after: None,
        };
        assert_eq!(none.retry_at(now), None);
    }

    #[test]
    fn http_fehler_ohne_rohen_body() {
        let html = "<!DOCTYPE html><html>Unicorn!</html>";
        let msg = describe_http_error(502, html);
        assert!(!msg.contains("html"), "{msg}");
        assert!(msg.contains("502"), "{msg}");

        let msg = describe_http_error(
            403,
            r#"{"message":"Resource not accessible by integration"}"#,
        );
        assert!(msg.contains("permissions"), "{msg}");
        assert!(msg.contains("Resource not accessible"), "{msg}");
        let msg = crate::l10n::with_lang(crate::l10n::Lang::De, || {
            describe_http_error(
                403,
                r#"{"message":"Resource not accessible by integration"}"#,
            )
        });
        assert!(msg.contains("Rechte"), "{msg}");
        assert!(msg.contains("„Resource not accessible"), "{msg}");

        let msg = describe_http_error(400, "kein json");
        assert!(!msg.contains("kein json"), "{msg}");
    }

    fn gql_error(message: &str, kind: Option<&str>) -> GqlError {
        GqlError {
            message: message.into(),
            path: Vec::new(),
            kind: kind.map(String::from),
        }
    }

    #[test]
    fn fehlende_schreibrechte_im_klartext() {
        let msg = describe_mutation_errors(&[gql_error(
            "Resource not accessible by personal access token",
            Some("FORBIDDEN"),
        )]);
        assert!(msg.contains("No write access"), "{msg}");
        assert!(!msg.contains("Resource not accessible"), "{msg}");

        let msg = describe_mutation_errors(&[gql_error(
            "octocat does not have the correct permissions to execute `MergePullRequest`",
            None,
        )]);
        assert!(msg.contains("No write access"), "{msg}");

        // Andere Fehler bleiben, wie sie sind – „Base branch was modified“ wird erneut versucht.
        let msg = describe_mutation_errors(&[
            gql_error(
                "Base branch was modified. Review and try the merge again.",
                None,
            ),
            gql_error("zweiter", None),
        ]);
        assert_eq!(
            msg,
            "Base branch was modified. Review and try the merge again.; zweiter"
        );
    }

    #[test]
    fn org_ohne_freigabe_im_klartext() {
        let raw = "Although you appear to have the correct authorization credentials, the `acme` \
                   organization has enabled OAuth App access restrictions, meaning that data access \
                   to third-parties is limited.";
        let msg = explain_access_error(raw, Some("FORBIDDEN")).unwrap();
        assert!(msg.contains("“acme”"), "{msg}");
        assert_eq!(explain_oauth_restriction(raw), Some(msg.clone()));
        assert!(msg.contains("Third-party access"), "{msg}");
        assert_eq!(
            explain_access_error("Could not resolve to a Repository", Some("NOT_FOUND")),
            None
        );
    }

    #[test]
    fn lesefehler_sind_keine_fehlenden_schreibrechte() {
        // Beim Abruf (Lesen) zählt nur die OAuth-Sperre der Org, sonst bleibt GitHubs Meldung.
        assert_eq!(
            explain_oauth_restriction("Resource protected by organization SAML enforcement."),
            None
        );
        assert_eq!(
            explain_oauth_restriction("Resource not accessible by personal access token"),
            None
        );
    }

    #[test]
    fn voruebergehende_fehler_beim_anmelden() {
        assert!(is_transient_status(502));
        assert!(is_transient_status(408));
        assert!(!is_transient_status(404));
        assert!(Transient::matches(&Transient("x".into()).into()));
        assert!(Transient::matches(
            &TimedOut(Duration::from_secs(30)).into()
        ));
        assert!(!Transient::matches(&anyhow!(
            "Die Anmeldung wurde abgelehnt"
        )));
    }

    #[test]
    fn angemeldeter_token_hat_vorrang() {
        let github = GitHub::new(Arc::new(gpui_kit::http_client::BlockedHttpClient));
        github.sign_in("gho_abc".into()).unwrap();
        assert_eq!(github.token_source(), Some(TokenSource::OAuth));
        assert_eq!(github.token().unwrap().value, "gho_abc");
    }

    #[test]
    fn abmelden_vergisst_den_token() {
        let github = GitHub::new(Arc::new(gpui_kit::http_client::BlockedHttpClient));
        github.sign_in("gho_abc".into()).unwrap();
        github.sign_out().unwrap();
        assert_eq!(github.token_source(), None);
    }

    #[test]
    fn spaete_401_loescht_keine_neue_anmeldung() {
        let github = GitHub::new(Arc::new(gpui_kit::http_client::BlockedHttpClient));
        github.sign_in("gho_neu".into()).unwrap();
        // Antwort einer Anfrage, die noch mit dem alten Token lief
        assert!(github.forget_rejected("gho_alt").is_none());
        assert_eq!(github.token().unwrap().value, "gho_neu");
        assert!(github.forget_rejected("gho_neu").is_none());
        assert_eq!(github.token_source(), None);
    }

    #[test]
    fn abgelehnte_anmeldung_wird_nicht_wieder_eingelesen() {
        assert_eq!(
            usable_saved(Some("gho_a".into()), None).as_deref(),
            Some("gho_a")
        );
        assert_eq!(usable_saved(Some("gho_a".into()), Some("gho_a")), None);
        assert_eq!(
            usable_saved(Some("gho_b".into()), Some("gho_a")).as_deref(),
            Some("gho_b")
        );
        assert_eq!(usable_saved(None, Some("gho_a")), None);
    }

    #[test]
    fn regeln_bleiben_bei_abruffehler_erhalten() {
        let mut cache = RulesCache::default();
        let key = ("a/b".to_string(), "main".to_string());
        let t0 = Instant::now();
        let rules = json!([{ "type": "pull_request", "parameters": { "allowed_merge_methods": ["merge"] } }]);
        let fresh = |etag: Option<&str>| RulesFetch::Fresh {
            rules: rules.clone(),
            etag: etag.map(String::from),
        };
        assert_eq!(
            cache.update(key.clone(), t0, fresh(Some("\"e1\""))),
            Some(rules.clone())
        );
        assert_eq!(cache.fresh(&key, t0), Some(rules.clone()));
        assert_eq!(cache.etag(&key).as_deref(), Some("\"e1\""));
        // abgelaufen → neu abrufen; Abruf scheitert → alter Stand gilt weiter
        let later = t0 + RULES_TTL + Duration::from_secs(1);
        assert_eq!(cache.fresh(&key, later), None);
        assert_eq!(
            cache.update(key.clone(), later, RulesFetch::Failed),
            Some(rules.clone())
        );
        assert_eq!(cache.fresh(&key, later), None);
        // 304 → alter Stand gilt wieder eine volle TTL lang
        assert_eq!(
            cache.update(key.clone(), later, RulesFetch::NotModified),
            Some(rules.clone())
        );
        assert_eq!(cache.fresh(&key, later), Some(rules));
        // nie erfolgreich geladen → keine Regeln bekannt
        let other = ("x/y".to_string(), "main".to_string());
        assert_eq!(cache.update(other.clone(), later, RulesFetch::Failed), None);
        assert_eq!(cache.update(other, later, RulesFetch::NotModified), None);
    }

    /// Antwort je Request (URI, Body) → Status, Header, Body.
    type Responder = fn(&str, &str) -> (u16, Vec<(&'static str, &'static str)>, String);

    /// Mitgeschriebene Requests: URI, `If-None-Match`, Body.
    type SentLog = Arc<Mutex<Vec<(String, Option<String>, String)>>>;

    /// Fake-Client, der Requests mitschreibt und je Request eine Antwort aus `respond` liefert.
    struct ScriptedClient {
        requests: SentLog,
        respond: Responder,
    }

    impl HttpClient for ScriptedClient {
        fn user_agent(&self) -> Option<&HeaderValue> {
            None
        }

        fn proxy(&self) -> Option<&Url> {
            None
        }

        fn send(
            &self,
            mut req: Request<AsyncBody>,
        ) -> BoxFuture<'static, Result<Response<AsyncBody>>> {
            let uri = req.uri().to_string();
            let if_none_match = req
                .headers()
                .get("If-None-Match")
                .and_then(|v| v.to_str().ok())
                .map(String::from);
            let (requests, respond) = (self.requests.clone(), self.respond);
            Box::pin(async move {
                let mut body = String::new();
                req.body_mut().read_to_string(&mut body).await?;
                let (status, headers, text) = respond(&uri, &body);
                requests.lock().unwrap().push((uri, if_none_match, body));
                let mut res = Response::builder().status(status);
                for (k, v) in headers {
                    res = res.header(k, v);
                }
                Ok(res.body(AsyncBody::from(text.into_bytes()))?)
            })
        }
    }

    fn scripted(respond: Responder) -> (GitHub, SentLog) {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let gh = GitHub::new(Arc::new(ScriptedClient {
            requests: requests.clone(),
            respond,
        }));
        *gh.token.lock().unwrap() = Some(Token {
            value: "test".into(),
            source: TokenSource::Env,
        });
        (gh, requests)
    }

    /// Antwort für ein Repo mit `total` offenen PRs, von denen `page` geliefert werden.
    fn repo_json(total: usize, page: usize) -> Value {
        let nodes: Vec<Value> = (0..total.min(page))
            .map(|i| {
                json!({
                    "id": format!("PR_{i}"), "number": i + 1, "title": "t", "url": "u",
                    "isDraft": false, "createdAt": "2026-01-01T00:00:00Z",
                    "updatedAt": "2026-01-01T00:00:00Z", "headRefName": "h", "headRefOid": "o",
                    "baseRefName": "main", "additions": 1, "deletions": 1
                })
            })
            .collect();
        json!({
            "nameWithOwner": "a/b", "url": "https://github.com/a/b",
            "open": { "totalCount": total, "nodes": nodes },
            "merged": { "nodes": [] }, "releases": { "nodes": [] }
        })
    }

    /// Seitengröße der offenen PRs aus einem Dashboard-Request lesen.
    fn page_in(body: &str) -> usize {
        let query: Value = serde_json::from_str(body).unwrap();
        let q = query["query"].as_str().unwrap();
        let rest = q.split("states: OPEN, first: ").nth(1).unwrap();
        rest.split(',').next().unwrap().parse().unwrap()
    }

    #[test]
    fn seite_schrumpft_nach_dem_ersten_abruf_und_waechst_bei_bedarf() {
        // Das Repo hat 12 offene PRs, ab dem dritten Poll 30.
        static TOTAL: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(12);
        let (gh, requests) = scripted(|uri, body| {
            if uri.contains("/rules/") {
                return (404, vec![], String::new());
            }
            let page = page_in(body);
            let total = TOTAL.load(std::sync::atomic::Ordering::SeqCst);
            let data = json!({ "data": {
                "viewer": { "login": "me" },
                "rateLimit": { "remaining": 4000, "limit": 5000, "resetAt": null, "cost": 2 },
                "r0": repo_json(total, page)
            }});
            (200, vec![], data.to_string())
        });
        let repos = vec!["a/b".to_string()];
        let pages = |requests: &SentLog| -> Vec<usize> {
            requests
                .lock()
                .unwrap()
                .iter()
                .filter(|(uri, _, _)| uri.ends_with("/graphql"))
                .map(|(_, _, b)| page_in(b))
                .collect()
        };

        let f = futures::executor::block_on(gh.fetch(&repos)).unwrap();
        assert_eq!(f.snapshot.open.len(), 12);
        assert_eq!(pages(&requests), [40]);

        let f = futures::executor::block_on(gh.fetch(&repos)).unwrap();
        assert_eq!(f.snapshot.open.len(), 12);
        assert_eq!(pages(&requests), [40, 15]);

        // Mehr PRs als die Seite fasst → sofort mit größerer Seite nachladen, nichts fehlt.
        TOTAL.store(30, std::sync::atomic::Ordering::SeqCst);
        requests.lock().unwrap().clear();
        let f = futures::executor::block_on(gh.fetch(&repos)).unwrap();
        assert_eq!(f.snapshot.open.len(), 30);
        assert_eq!(pages(&requests), [15, 35]);
        // Kosten beider Requests zählen für diesen Poll.
        assert_eq!(f.rate_limit.map(|r| r.cost), Some(4));
    }

    #[test]
    fn kontingent_mehrerer_requests_wird_zusammengefasst() {
        let rl = |remaining, cost| RateLimit {
            remaining,
            limit: 5000,
            reset_at: None,
            cost,
        };
        let merged = merge_rate_limits(Some(rl(4000, 3)), Some(rl(3990, 2))).unwrap();
        assert_eq!((merged.remaining, merged.cost), (3990, 5));
        let merged = merge_rate_limits(Some(rl(3990, 3)), Some(rl(4000, 2))).unwrap();
        assert_eq!((merged.remaining, merged.cost), (3990, 5));
        assert_eq!(merge_rate_limits(None, Some(rl(1, 1))), Some(rl(1, 1)));
        assert_eq!(merge_rate_limits(None, None), None);
    }

    #[test]
    fn branch_regeln_fragen_mit_etag_nach() {
        let (gh, requests) = scripted(|_, _| {
            (
                200,
                vec![("etag", "\"abc\"")],
                r#"[{"type":"deletion"}]"#.to_string(),
            )
        });
        let got = futures::executor::block_on(gh.fetch_branch_rules("a/b", "main", None));
        assert_eq!(
            got,
            RulesFetch::Fresh {
                rules: json!([{ "type": "deletion" }]),
                etag: Some("\"abc\"".into())
            }
        );
        let (gh304, requests304) = scripted(|_, _| (304, vec![], String::new()));
        let got = futures::executor::block_on(gh304.fetch_branch_rules(
            "a/b",
            "feature/x",
            Some("\"abc\""),
        ));
        assert_eq!(got, RulesFetch::NotModified);
        let sent = requests304.lock().unwrap();
        assert_eq!(sent[0].1.as_deref(), Some("\"abc\""));
        assert!(sent[0].0.ends_with("/rules/branches/feature%2Fx"));
        assert_eq!(requests.lock().unwrap()[0].1, None);
    }
}
