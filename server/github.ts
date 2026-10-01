import { execFile } from "node:child_process";
import { promisify } from "node:util";

const exec = promisify(execFile);

let cached: { token: string; source: "env" | "gh-cli" } | null = null;

/** Token aus GITHUB_TOKEN/GH_TOKEN, sonst aus der GitHub-CLI (`gh auth token`). */
export async function getToken(): Promise<{ token: string; source: "env" | "gh-cli" }> {
  if (cached) return cached;
  const env = process.env.GITHUB_TOKEN || process.env.GH_TOKEN;
  if (env) return (cached = { token: env.trim(), source: "env" });
  try {
    const { stdout } = await exec("gh", ["auth", "token"], { timeout: 5000 });
    const token = stdout.trim();
    if (token) return (cached = { token, source: "gh-cli" });
  } catch {
    /* gh nicht installiert oder nicht eingeloggt */
  }
  throw new Error(
    "Kein GitHub-Token gefunden. Setze GITHUB_TOKEN in .env oder melde dich mit `gh auth login` an.",
  );
}

export function tokenSource() {
  return cached?.source ?? null;
}

export class GraphQLError extends Error {
  constructor(
    message: string,
    public errors: { message: string; path?: (string | number)[]; type?: string }[] = [],
  ) {
    super(message);
  }
}

export interface GraphQLResult<T> {
  data: T | null;
  errors: { message: string; path?: (string | number)[]; type?: string }[];
}

/** REST-GET gegen die GitHub-API; `null`, wenn nicht abrufbar (z. B. fehlende Rechte). */
export async function rest<T>(path: string): Promise<T | null> {
  const { token } = await getToken();
  const res = await fetch(`https://api.github.com${path}`, {
    headers: { Authorization: `bearer ${token}`, Accept: "application/vnd.github+json", "User-Agent": "pr-radar" },
    signal: AbortSignal.timeout(15_000),
  }).catch(() => null);
  if (!res?.ok) return null;
  return (await res.json()) as T;
}

export async function graphql<T>(query: string, variables: Record<string, unknown> = {}): Promise<GraphQLResult<T>> {
  const { token } = await getToken();
  const res = await fetch(process.env.GITHUB_GRAPHQL_URL ?? "https://api.github.com/graphql", {
    method: "POST",
    headers: {
      Authorization: `bearer ${token}`,
      "Content-Type": "application/json",
      "User-Agent": "pr-radar",
    },
    body: JSON.stringify({ query, variables }),
    signal: AbortSignal.timeout(30_000),
  });
  if (res.status === 401) {
    cached = null;
    throw new GraphQLError("GitHub lehnt den Token ab (401). Token prüfen.");
  }
  if (!res.ok) throw new GraphQLError(`GitHub API ${res.status}: ${await res.text().catch(() => "")}`);
  const json = (await res.json()) as { data?: T; errors?: GraphQLResult<T>["errors"] };
  return { data: json.data ?? null, errors: json.errors ?? [] };
}
