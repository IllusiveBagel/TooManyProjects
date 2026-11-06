import { describe, expect, it } from "vitest";
import { dueLabel, lastActive, progress } from "./format";

const now = new Date(2026, 10, 6, 15, 0); // Fri 6 Nov 2026, local time

describe("lastActive", () => {
  it("buckets by calendar day", () => {
    expect(lastActive(new Date(2026, 10, 6, 1).toISOString(), now)).toBe("Today");
    expect(lastActive(new Date(2026, 10, 5, 23).toISOString(), now)).toBe("Yesterday");
    expect(lastActive(new Date(2026, 10, 1).toISOString(), now)).toBe("5d ago");
    expect(lastActive(new Date(2026, 9, 16).toISOString(), now)).toBe("3w ago");
    expect(lastActive(new Date(2026, 5, 1).toISOString(), now)).toBe("5mo ago");
    expect(lastActive(new Date(2024, 10, 1).toISOString(), now)).toBe("2y ago");
  });
});

describe("dueLabel", () => {
  it("is relative this week and a date after", () => {
    expect(dueLabel("2026-11-05", now)).toBe("Overdue");
    expect(dueLabel("2026-11-06", now)).toBe("Due today");
    expect(dueLabel("2026-11-10", now)).toBe("Due Tue");
    expect(dueLabel("2026-12-24", now)).toBe("Due 24 Dec");
    expect(dueLabel("2027-03-01", now)).toBe("Due 1 Mar 27");
  });
});

describe("progress", () => {
  const stats = (done: number, total: number) => ({ stats: { done_tasks: done, total_tasks: total } }) as never;
  it("rounds and handles no tasks", () => {
    expect(progress(stats(0, 0))).toBe(0);
    expect(progress(stats(17, 45))).toBe(38);
    expect(progress(stats(4, 4))).toBe(100);
  });
});
