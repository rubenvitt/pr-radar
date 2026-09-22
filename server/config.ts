import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";

const FILE = resolve(process.env.DATA_DIR ?? "data", "config.json");
const REPO_RE = /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/;

export interface Config {
  repos: string[];
}

/** Akzeptiert "owner/name", "https://github.com/owner/name(.git)(/...)" und "git@github.com:owner/name.git". */
export function parseRepo(input: string): string | null {
  let s = input.trim();
  s = s.replace(/^git@github\.com:/, "").replace(/^https?:\/\/(www\.)?github\.com\//, "");
  s = s.replace(/\.git$/, "");
  const [owner, name] = s.split("/");
  const full = owner && name ? `${owner}/${name.replace(/\.git$/, "")}` : "";
  return REPO_RE.test(full) ? full : null;
}

let current: Config | null = null;

export async function loadConfig(): Promise<Config> {
  if (current) return current;
  try {
    current = JSON.parse(await readFile(FILE, "utf8")) as Config;
  } catch {
    const fromEnv = (process.env.REPOS ?? "")
      .split(",")
      .map((r) => parseRepo(r))
      .filter((r): r is string => !!r);
    current = { repos: fromEnv };
    await saveConfig(current);
  }
  return current;
}

export async function saveConfig(cfg: Config): Promise<void> {
  current = cfg;
  await mkdir(dirname(FILE), { recursive: true });
  await writeFile(FILE, JSON.stringify(cfg, null, 2) + "\n");
}
