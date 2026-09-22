import { describe, expect, it } from "vitest";
import { buildPipeline, normalizeRepo, rollupState, type RawRepo } from "./normalize";
import { parseRepo } from "./config";
import { buildDashboardQuery } from "./queries";

const run = (status: string, conclusion: string | null, name = "ci") => ({ __typename: "CheckRun" as const, name, status, conclusion, detailsUrl: null });

describe("buildPipeline", () => {
  it("ist 'none' ohne Checks", () => {
    expect(buildPipeline(null).state).toBe("none");
  });
  it("rot schlägt laufend", () => {
    const p = buildPipeline({ state: "PENDING", contexts: { nodes: [run("IN_PROGRESS", null, "a"), run("COMPLETED", "FAILURE", "b")] } });
    expect(p).toMatchObject({ state: "failed", failed: 1, running: 1, total: 2 });
    expect(p.checks[0].name).toBe("b"); // Fehler zuerst
  });
  it("laufend, solange etwas nicht fertig ist", () => {
    expect(buildPipeline({ state: "PENDING", contexts: { nodes: [run("QUEUED", null), run("COMPLETED", "SUCCESS")] } }).state).toBe("running");
  });
  it("grün mit übersprungenen Checks", () => {
    expect(buildPipeline({ state: "SUCCESS", contexts: { nodes: [run("COMPLETED", "SUCCESS"), run("COMPLETED", "SKIPPED")] } }).state).toBe("passed");
  });
  it("versteht klassische Commit-Statuses", () => {
    const p = buildPipeline({ state: "FAILURE", contexts: { nodes: [{ __typename: "StatusContext", context: "ci/jenkins", state: "ERROR", targetUrl: "x" }] } });
    expect(p.state).toBe("failed");
  });
  it("mappt Rollup-States", () => {
    expect(rollupState("EXPECTED")).toBe("running");
    expect(rollupState(undefined)).toBe("none");
  });
});

describe("parseRepo", () => {
  it.each([
    ["rubeen/pr-radar", "rubeen/pr-radar"],
    ["https://github.com/rubeen/pr-radar", "rubeen/pr-radar"],
    ["https://github.com/rubeen/pr-radar/pulls", "rubeen/pr-radar"],
    ["git@github.com:rubeen/pr-radar.git", "rubeen/pr-radar"],
    ["kaputt", null],
    ["a/b c", null],
  ])("%s → %s", (input, out) => expect(parseRepo(input)).toBe(out));
});

describe("buildDashboardQuery", () => {
  it("aliast Repos und escaped Namen", () => {
    const q = buildDashboardQuery(["a/b", "c/d"]);
    expect(q).toContain('r0: repository(owner: "a", name: "b")');
    expect(q).toContain('r1: repository(owner: "c", name: "d")');
  });
});

describe("normalizeRepo", () => {
  const raw = (flags: Partial<RawRepo>) =>
    ({ nameWithOwner: "a/b", url: "u", autoMergeAllowed: true, squashMergeAllowed: false, mergeCommitAllowed: false, rebaseMergeAllowed: false, ...flags }) as RawRepo;
  it("liefert nur die im Repo erlaubten Merge-Methoden", () => {
    expect(normalizeRepo(raw({ squashMergeAllowed: true })).mergeMethods).toEqual(["SQUASH"]);
    expect(normalizeRepo(raw({ mergeCommitAllowed: true, rebaseMergeAllowed: true })).mergeMethods).toEqual(["MERGE", "REBASE"]);
    expect(normalizeRepo(raw({})).mergeMethods).toEqual([]);
  });
});
