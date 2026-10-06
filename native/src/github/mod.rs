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
}

#[derive(Deserialize, Debug)]
struct GqlResponse {
    data: Option<HashMap<String, Value>>,
    #[serde(default)]
    errors: Vec<GqlError>,
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
        let mut text = String::new();
        res.body_mut().read_to_string(&mut text).await?;
        if status.as_u16() == 401 {
            self.forget_token();
            bail!("GitHub lehnt den Token ab (401). Token prüfen.");
        }
        if !status.is_success() {
            bail!(
                "GitHub API {}: {}",
                status.as_u16(),
                text.chars().take(300).collect::<String>()
            );
        }
        serde_json::from_str(&text).context("Unerwartete Antwort von GitHub")
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
