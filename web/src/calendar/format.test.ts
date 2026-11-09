import { describe, expect, it } from "vitest";
import { addDays, hoursText, placeDay, repeatText, snap, summarise, weekStart, type Occurrence } from "./format";

function occ(start: Date, minutes: number, extra: Partial<Occurrence["event"]> = {}, joined = false): Occurrence {
  const end = new Date(start.getTime() + minutes * 60_000);
  return {
    occurrence: "2026-11-09",
    start_at: start.toISOString(),
    end_at: end.toISOString(),
    joiners: [],
    joined,
    can_join: false,
    event: {
      id: Math.random().toString(36),
      user_id: "me",
      user_name: "Logan",
      kind: "session",
      project_id: "car",
      project_title: "Beetle",
      project_color: "slot-1",
      title: null,
      start_local: "2026-11-09T19:00",
      minutes,
      tz: "UTC",
      repeat_days: 0,
      repeat_until: null,
      via: "web",
      created_at: "",
      updated_at: "",
      can_edit: true,
      ...extra,
    },
  };
}

const monday = new Date(2026, 10, 9);
const at = (dayOffset: number, h: number, m = 0) => new Date(2026, 10, 9 + dayOffset, h, m);

describe("weeks", () => {
  it("start on Monday", () => {
    expect(weekStart(new Date(2026, 10, 15, 22)).getDate()).toBe(9);
    expect(weekStart(new Date(2026, 10, 9, 1)).getDate()).toBe(9);
    expect(addDays(monday, 6).getDate()).toBe(15);
  });
  it("say hours and repeats plainly", () => {
    expect(hoursText(90)).toBe("1h 30");
    expect(hoursText(240)).toBe("4h");
    expect(hoursText(45)).toBe("45m");
    expect(repeatText(31)).toBe("weekdays");
    expect(repeatText(32)).toBe("Sat");
    expect(repeatText(0)).toBe("");
    expect(snap(37)).toBe(30);
  });
});

describe("placeDay", () => {
  it("puts overlapping blocks side by side and others full width", () => {
    const work = occ(at(0, 9), 480, { kind: "busy", project_id: null });
    const lunch = occ(at(0, 12), 60);
    const evening = occ(at(0, 19), 120);
    const placed = placeDay(monday, [evening, work, lunch]);
    const byId = (o: Occurrence) => placed.find((p) => p.occ === o)!;
    expect(byId(work)).toMatchObject({ top: 540, bottom: 1020, lane: 0, lanes: 2 });
    expect(byId(lunch)).toMatchObject({ lane: 1, lanes: 2 });
    expect(byId(evening)).toMatchObject({ lane: 0, lanes: 1 });
  });
  it("clips a block that runs past midnight", () => {
    const late = occ(at(0, 22), 240);
    expect(placeDay(monday, [late])[0]).toMatchObject({ top: 1320, bottom: 1440 });
    expect(placeDay(addDays(monday, 1), [late])[0]).toMatchObject({ top: 0, bottom: 120 });
  });
});

describe("summarise", () => {
  it("counts your sessions per project and what's left of the waking week", () => {
    const list = [
      occ(at(0, 19), 120),
      occ(at(5, 13), 240),
      occ(at(3, 19), 180, { project_id: "blender", project_title: "Blender", user_id: "sam" }, true),
      occ(at(6, 10), 120, { project_id: "garden", project_title: "Garden", user_id: "sam" }),
      ...[0, 1, 2, 3, 4].map((d) => occ(at(d, 9), 480, { kind: "busy", project_id: null })),
    ];
    const s = summarise(monday, list, "me");
    expect(s.planned).toBe(540);
    expect(s.perProject.map((p) => [p.id, p.minutes])).toEqual([
      ["car", 360],
      ["blender", 180],
    ]);
    // 105 waking hours, less 40 at work and 9 planned. Sam's unjoined garden session doesn't count.
    expect(s.free).toBe((105 - 40 - 9) * 60);
  });
});
