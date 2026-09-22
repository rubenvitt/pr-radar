import type { ReactNode } from "react";
import type { PipelineState, Person } from "../../../shared/types";
import { CircleCheck, CircleDashed, CircleX, LoaderCircle } from "lucide-react";

export function cx(...c: (string | false | null | undefined)[]) {
  return c.filter(Boolean).join(" ");
}

/** Stabile Farbe pro Repo, damit man Repos in gemischten Listen schnell erkennt. */
export function repoHue(repo: string) {
  let h = 0;
  for (const ch of repo) h = (h * 31 + ch.charCodeAt(0)) % 360;
  return h;
}

export function RepoTag({ repo, short = true }: { repo: string; short?: boolean }) {
  const hue = repoHue(repo);
  const [owner, name] = repo.split("/");
  return (
    <span className="inline-flex items-center gap-1.5 text-xs text-muted whitespace-nowrap" title={repo}>
      <span className="size-2 rounded-full shrink-0" style={{ background: `oklch(0.68 0.15 ${hue})` }} />
      {short ? name : (<><span className="opacity-70">{owner}/</span>{name}</>)}
    </span>
  );
}

export function Avatar({ person, size = 20 }: { person: Person | null; size?: number }) {
  if (!person) return <span className="rounded-full bg-panel-2 inline-block" style={{ width: size, height: size }} />;
  return (
    <img
      src={`${person.avatarUrl}${person.avatarUrl.includes("?") ? "&" : "?"}s=${size * 2}`}
      alt={person.login}
      title={person.login}
      width={size}
      height={size}
      className="rounded-full ring-1 ring-line shrink-0"
      loading="lazy"
    />
  );
}

export function Badge({ children, tone = "neutral", title }: { children: ReactNode; tone?: "neutral" | "pass" | "fail" | "run" | "accent"; title?: string }) {
  const tones = {
    neutral: "bg-panel-2 text-muted",
    pass: "bg-pass/12 text-pass",
    fail: "bg-fail/12 text-fail",
    run: "bg-run/14 text-run",
    accent: "bg-accent/12 text-accent",
  };
  return (
    <span title={title} className={cx("inline-flex items-center gap-1 rounded-md px-1.5 py-0.5 text-[11px] font-medium whitespace-nowrap", tones[tone])}>
      {children}
    </span>
  );
}

export function PipelineIcon({ state, size = 18 }: { state: PipelineState | "skipped" | "neutral"; size?: number }) {
  switch (state) {
    case "running":
      return <LoaderCircle size={size} className="text-run spin-slow shrink-0" aria-label="läuft" />;
    case "failed":
      return <CircleX size={size} className="text-fail shrink-0" aria-label="fehlgeschlagen" />;
    case "passed":
      return <CircleCheck size={size} className="text-pass shrink-0" aria-label="erfolgreich" />;
    default:
      return <CircleDashed size={size} className="text-muted/60 shrink-0" aria-label="keine Checks" />;
  }
}

export function Empty({ icon, title, children }: { icon: ReactNode; title: string; children?: ReactNode }) {
  return (
    <div className="flex flex-col items-center justify-center py-20 text-center">
      <div className="mb-3 text-muted">{icon}</div>
      <div className="font-medium">{title}</div>
      {children && <div className="mt-1 max-w-sm text-sm text-muted">{children}</div>}
    </div>
  );
}
