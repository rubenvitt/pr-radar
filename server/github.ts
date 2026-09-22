import { execFile } from "node:child_process";
import { promisify } from "node:util";

const exec = promisify(execFile);

export type TokenSource = "env" | "gh-cli" | "client";
export interface ServerToken {
  token: string;
  source: "env" | "gh-cli";
}

export const NO_SERVER_TOKEN =
  "Kein GitHub-Token auf dem Server. Setze GITHUB_TOKEN, melde dich mit `gh auth login` an – " +
  "oder hinterlege unter ⚙︎ einen persönlichen Token im Browser.";

let cached: ServerToken | null = null;
/** Ergebnislose Suche kurz merken, damit nicht jeder Poll `gh` aufruft. */
let retryAt = 0;

/** Token aus GITHUB_TOKEN/GH_TOKEN, sonst aus der GitHub-CLI (`gh auth token`). `null` = keiner vorhanden. */
export async function findServerToken(): Promise<ServerToken | null> {
  if (cached) return cached;
  if (Date.now() < retryAt) return null;
  const env = (process.env.GITHUB_TOKEN || process.env.GH_TOKEN || "").trim();
  if (env) return (cached = { token: env, source: "env" });
  try {
    const { stdout } = await exec("gh", ["auth", "token"], { timeout: 5000 });
    const token = stdout.trim();
    if (token) return (cached = { token, source: "gh-cli" });
  } catch {
    /* gh nicht installiert oder nicht eingeloggt */
  }
  retryAt = Date.now() + 60_000;
  return null;
}

export async function getToken(): Promise<ServerToken> {
  const found = await findServerToken();
  if (!found) throw new GraphQLError(NO_SERVER_TOKEN, [], 401);
  return found;
}

export function tokenSource() {
  return cached?.source ?? null;
}

/**
 * Prüft einen vom Browser mitgeschickten Token grob auf Form (druckbares ASCII, keine Leerzeichen).
 * Der Token wird ausschließlich für den laufenden Request verwendet und nie gespeichert.
 */
export function parseClientToken(raw: string | undefined | null): string | null {
  const value = raw?.trim() ?? "";
  if (!value) return null;
  return /^[\x21-\x7e]{8,512}$/.test(value) ? value : null;
}

export class GraphQLError extends Error {
  constructor(
    message: string,
    public errors: { message: string; path?: (string | number)[]; type?: string }[] = [],
    public status = 502,
  ) {
    super(message);
  }
}

export interface GraphQLResult<T> {
  data: T | null;
  errors: { message: string; path?: (string | number)[]; type?: string }[];
}

/** GraphQL-Request. Mit `token` wird der Browser-Token genutzt, ohne der Token des Servers. */
export async function graphql<T>(
  query: string,
  variables: Record<string, unknown> = {},
  token?: string,
): Promise<GraphQLResult<T>> {
  const auth = token ?? (await getToken()).token;
  const res = await fetch(process.env.GITHUB_GRAPHQL_URL ?? "https://api.github.com/graphql", {
    method: "POST",
    headers: {
      Authorization: `bearer ${auth}`,
      "Content-Type": "application/json",
      "User-Agent": "pr-radar",
    },
    body: JSON.stringify({ query, variables }),
    signal: AbortSignal.timeout(30_000),
  });
  if (res.status === 401) {
    if (!token) cached = null;
    throw new GraphQLError(
      token ? "GitHub lehnt den im Browser hinterlegten Token ab (401)." : "GitHub lehnt den Token ab (401). Token prüfen.",
      [],
      401,
    );
  }
  if (!res.ok) throw new GraphQLError(`GitHub API ${res.status}: ${await res.text().catch(() => "")}`);
  const json = (await res.json()) as { data?: T; errors?: GraphQLResult<T>["errors"] };
  return { data: json.data ?? null, errors: json.errors ?? [] };
}
