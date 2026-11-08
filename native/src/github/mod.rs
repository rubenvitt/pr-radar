//! GitHub-GraphQL-Client: Token-Suche, Requests, Dashboard-Abfrage, Mutationen.

mod normalize;
mod queries;

use std::collections::HashMap;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenSource {
    Env,
    GhCli,
}

impl TokenSource {
    pub fn label(self) -> &'static str {
        match self {
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
            Some(at) => write!(
                f,
                "GitHub-API-Kontingent aufgebraucht. Wieder frei um {} Uhr.",
                at.with_timezone(&chrono::Local).format("%H:%M")
            ),
            None => write!(
                f,
                "GitHub drosselt gerade die Anfragen. Neuer Versuch in ein paar Minuten."
            ),
        }
    }
}

impl std::error::Error for RateLimited {}

/// Drosselung ohne Zeitangabe: so lange warten.
pub const UNKNOWN_RATE_LIMIT_WAIT: Duration = Duration::from_secs(120);
/// Sekundäres Limit ohne `Retry-After`: GitHub empfiehlt mindestens eine Minute Pause.
const SECONDARY_RATE_LIMIT_WAIT: Duration = Duration::from_secs(60);

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

/// Liest die Drossel-Angaben aus einer Antwort, falls GitHub drosselt.
fn rate_limited(
    status: u16,
    headers: &gpui_kit::http_client::http::HeaderMap,
    body: &str,
) -> Option<RateLimited> {
    let h = LimitHeaders::read(headers);
    let secondary = status == 403 && mentions_secondary_limit(body);
    let throttled =
        status == 429 || (status == 403 && (h.exhausted || h.retry_after.is_some() || secondary));
    if !throttled {
        return None;
    }
    let mut limited = h.limited();
    if secondary && limited.retry_after.is_none() && limited.reset_at.is_none() {
        limited.retry_after = Some(SECONDARY_RATE_LIMIT_WAIT);
    }
    Some(limited)
}

/// Verständlicher Text für einen HTTP-Fehler statt des rohen Antwort-Bodys.
fn describe_http_error(status: u16, body: &str) -> String {
    // GitHub liefert meist `{"message": "...", "documentation_url": "..."}`.
    let detail = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v["message"].as_str().map(str::to_owned))
        .filter(|m| !m.trim().is_empty())
        .map(|m| {
            format!(
                " GitHub meldet: „{}“",
                m.chars().take(160).collect::<String>()
            )
        })
        .unwrap_or_default();
    let text = match status {
        403 => "GitHub verweigert den Zugriff (403). Fehlen dem Token Rechte für diese Repos?",
        404 => "GitHub kennt die Adresse nicht (404).",
        408 | 504 => "GitHub hat nicht rechtzeitig geantwortet. Neuer Versuch folgt automatisch.",
        500..=599 => "GitHub hat gerade Probleme. Neuer Versuch folgt automatisch.",
        _ => "GitHub hat die Anfrage abgelehnt.",
    };
    if (500..=599).contains(&status) {
        format!("{text} (HTTP {status})")
    } else {
        format!("{text}{detail}")
    }
}

/// Ergebnis eines Abrufs aller Repos.
pub struct Fetched {
    pub snapshot: Snapshot,
    pub rate_limit: Option<RateLimit>,
}

pub struct GitHub {
    http: Arc<dyn HttpClient>,
    token: Mutex<Option<Token>>,
    rules: Mutex<RulesCache>,
}

/// Wie lange Branch-Regeln gültig bleiben – sie ändern sich selten, kosten aber REST-Kontingent.
const RULES_TTL: Duration = Duration::from_secs(600);

/// Branch-Regeln je (Repo, Branch). Bei Abruffehlern gilt der letzte bekannte Stand weiter,
/// damit nicht still wieder alle Merge-Methoden angeboten werden.
#[derive(Default)]
struct RulesCache {
    entries: HashMap<(String, String), (Instant, Value)>,
}

impl RulesCache {
    fn fresh(&self, key: &(String, String), now: Instant) -> Option<Value> {
        self.entries
            .get(key)
            .filter(|(at, _)| now.duration_since(*at) < RULES_TTL)
            .map(|(_, v)| v.clone())
    }

    /// Übernimmt ein Abrufergebnis; ohne Ergebnis bleibt der alte Eintrag gültig.
    fn update(
        &mut self,
        key: (String, String),
        now: Instant,
        fetched: Option<Value>,
    ) -> Option<Value> {
        match fetched {
            Some(v) => {
                self.entries.insert(key, (now, v.clone()));
                Some(v)
            }
            None => self.entries.get(&key).map(|(_, v)| v.clone()),
        }
    }
}

impl GitHub {
    pub fn new(http: Arc<dyn HttpClient>) -> Self {
        Self {
            http,
            token: Mutex::new(None),
            rules: Mutex::new(RulesCache::default()),
        }
    }

    pub fn token_source(&self) -> Option<TokenSource> {
        self.token.lock().ok()?.as_ref().map(|t| t.source)
    }

    /// Token aus `GITHUB_TOKEN`/`GH_TOKEN`, sonst aus der GitHub-CLI. Blockiert – nur im Hintergrund aufrufen.
    fn token(&self) -> Result<Token> {
        if let Some(t) = self
            .token
            .lock()
            .map_err(|_| anyhow!("Token-Cache gesperrt"))?
            .clone()
        {
            return Ok(t);
        }
        let token = find_token().ok_or_else(|| {
            anyhow!("Kein GitHub-Token gefunden. Setze GITHUB_TOKEN oder melde dich mit `gh auth login` an.")
        })?;
        *self
            .token
            .lock()
            .map_err(|_| anyhow!("Token-Cache gesperrt"))? = Some(token.clone());
        Ok(token)
    }

    fn forget_token(&self) {
        if let Ok(mut t) = self.token.lock() {
            *t = None;
        }
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
        let mut res = self
            .http
            .send(req)
            .await
            .context("GitHub nicht erreichbar")?;
        let status = res.status();
        let headers = res.headers().clone();
        let mut text = String::new();
        // Erst die Drosselung prüfen: Status und Header reichen dafür auch, wenn der Body
        // nicht lesbar ist – sonst ginge die Wartezeit verloren.
        let read = res.body_mut().read_to_string(&mut text).await;
        if let Some(limited) = rate_limited(status.as_u16(), &headers, &text) {
            return Err(limited.into());
        }
        read?;
        if status.as_u16() == 401 {
            self.forget_token();
            bail!("GitHub lehnt den Token ab (401). Token prüfen.");
        }
        if !status.is_success() {
            bail!("{}", describe_http_error(status.as_u16(), &text));
        }
        let res: GqlResponse =
            serde_json::from_str(&text).context("Unerwartete Antwort von GitHub")?;
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
        let res = self.request(query, variables).await?;
        if !res.errors.is_empty() {
            bail!(
                "{}",
                res.errors
                    .iter()
                    .map(|e| e.message.as_str())
                    .collect::<Vec<_>>()
                    .join("; ")
            );
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
        let mut snapshot = Snapshot::default();
        let mut rate_limit = None;
        let chunks: Vec<&[String]> = repos.chunks(CHUNK).collect();
        let results =
            futures::future::join_all(chunks.iter().map(|c| self.request_dashboard(c))).await;
        // Ein Rate-Limit in irgendeinem Chunk zählt vor anderen Fehlern, damit der
        // Backoff bis zum Reset wartet statt zu früh erneut anzufragen.
        if let Some(limited) = longest_rate_limit(&results, Utc::now()) {
            return Err(limited.into());
        }

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
                        "Leere Antwort von GitHub".into()
                    } else {
                        msg
                    }
                );
            };
            if snapshot.viewer.is_none() {
                snapshot.viewer = data
                    .get("viewer")
                    .and_then(|v| v["login"].as_str())
                    .map(String::from);
            }
            if let Some(rl) = data.get("rateLimit") {
                rate_limit = Some(RateLimit {
                    remaining: rl["remaining"].as_u64().unwrap_or(0),
                    limit: rl["limit"].as_u64().unwrap_or(0),
                    reset_at: rl["resetAt"].as_str().and_then(|s| s.parse().ok()),
                });
            }
            for (i, full) in chunk.iter().enumerate() {
                let alias = format!("r{i}");
                let raw = data.get(&alias).filter(|v| !v.is_null()).cloned();
                match raw.map(serde_json::from_value::<RawRepo>) {
                    Some(Ok(raw)) => normalize::push_repo(&mut snapshot, &raw),
                    Some(Err(e)) => snapshot.repos.push(RepoInfo::unavailable(
                        full,
                        format!("Antwort nicht lesbar: {e}"),
                    )),
                    None => {
                        let err = res.errors.iter().find(|e| {
                            e.path.first().and_then(Value::as_str) == Some(alias.as_str())
                        });
                        let msg = err.map(|e| e.message.clone()).unwrap_or_else(|| {
                            "Repository nicht gefunden oder kein Zugriff".into()
                        });
                        snapshot.repos.push(RepoInfo::unavailable(full, msg));
                    }
                }
            }
        }

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
        if let Some(v) = self.rules.lock().ok()?.fresh(&key, Instant::now()) {
            return Some(v);
        }
        let fetched = self.fetch_branch_rules(repo, branch).await;
        self.rules.lock().ok()?.update(key, Instant::now(), fetched)
    }

    async fn fetch_branch_rules(&self, repo: &str, branch: &str) -> Option<Value> {
        let token = self.token().ok()?;
        let req = Request::builder()
            .uri(format!(
                "https://api.github.com/repos/{repo}/rules/branches/{}",
                encode_path(branch)
            ))
            .header("Authorization", format!("bearer {}", token.value))
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "pr-radar-native")
            .body(AsyncBody::empty())
            .ok()?;
        let mut res = self.http.send(req).await.ok()?;
        if !res.status().is_success() {
            return None;
        }
        let mut text = String::new();
        res.body_mut().read_to_string(&mut text).await.ok()?;
        serde_json::from_str(&text).ok()
    }

    async fn request_dashboard(&self, chunk: &[String]) -> Result<GqlResponse> {
        self.request(&queries::dashboard_query(chunk), json!({}))
            .await
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

fn find_token() -> Option<Token> {
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
    fn sekundaeres_limit_ohne_retry_after_wartet_eine_minute() {
        let h = headers(&[("x-ratelimit-remaining", "4000")]);
        let body = r#"{"message":"You have exceeded a secondary rate limit. Please wait a few minutes before you try again."}"#;
        let limited = rate_limited(403, &h, body).expect("gedrosselt");
        assert_eq!(limited.reset_at, None);
        assert_eq!(limited.retry_after, Some(SECONDARY_RATE_LIMIT_WAIT));
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
        assert!(msg.contains("Rechte"), "{msg}");
        assert!(msg.contains("Resource not accessible"), "{msg}");

        let msg = describe_http_error(400, "kein json");
        assert!(!msg.contains("kein json"), "{msg}");
    }

    #[test]
    fn regeln_bleiben_bei_abruffehler_erhalten() {
        let mut cache = RulesCache::default();
        let key = ("a/b".to_string(), "main".to_string());
        let t0 = Instant::now();
        let rules = json!([{ "type": "pull_request", "parameters": { "allowed_merge_methods": ["merge"] } }]);
        assert_eq!(
            cache.update(key.clone(), t0, Some(rules.clone())),
            Some(rules.clone())
        );
        assert_eq!(cache.fresh(&key, t0), Some(rules.clone()));
        // abgelaufen → neu abrufen; Abruf scheitert → alter Stand gilt weiter
        let later = t0 + RULES_TTL + Duration::from_secs(1);
        assert_eq!(cache.fresh(&key, later), None);
        assert_eq!(cache.update(key.clone(), later, None), Some(rules));
        // nie erfolgreich geladen → keine Regeln bekannt
        assert_eq!(
            cache.update(("x/y".into(), "main".into()), later, None),
            None
        );
    }
}
