import { describe, expect, it } from "vitest";
import { byWeek, fileKind, fileSize, tileLabel, type Attachment } from "./format";

const a = (over: Partial<Attachment>): Attachment => ({
  id: "a",
  name: "IMG_1.jpg",
  mime: "image/jpeg",
  size: 1,
  is_image: true,
  via: "web",
  created_at: "2026-11-10T09:00:00Z",
  can_edit: true,
  ...over,
});

describe("files", () => {
  it("sizes and kinds read like the mockups", () => {
    expect(fileSize(2.4 * 1024 * 1024)).toBe("2.4 MB");
    expect(fileSize(18 * 1024 * 1024)).toBe("18 MB");
    expect(fileSize(3 * 1024)).toBe("3 KB");
    expect(fileKind({ name: "rex_helmet_v3.stl", mime: "model/stl" })).toBe("STL");
    expect(fileKind({ name: "noext", mime: "application/pdf" })).toBe("PDF");
    expect(fileKind({ name: "noext", mime: "application/octet-stream" })).toBe("FILE");
  });

  it("labels tiles by caption, task or note", () => {
    expect(tileLabel(a({ task_number: 5, task_title: "Strip Interior" }))).toBe("T-5 Strip Interior");
    expect(tileLabel(a({ task_number: 5, task_title: "Strip Interior", caption: "Loom labels" }))).toBe("T-5 Loom labels");
    expect(tileLabel(a({ task_number: 5, caption: "Seats out" }), false)).toBe("Seats out");
    expect(tileLabel(a({ note_title: "Wiring" }))).toBe("Note: Wiring");
    expect(tileLabel(a({}))).toBe("IMG_1");
  });

  it("groups a build log by week, newest first, using when the photo was taken", () => {
    const now = new Date(2026, 10, 12, 12); // Thu 12 Nov 2026
    const groups = byWeek(
      [
        a({ id: "old", created_at: "2026-10-01T10:00:00Z" }),
        a({ id: "taken", created_at: "2026-11-11T10:00:00Z", taken_at: "2026-11-06T16:20:00" }),
        a({ id: "new", created_at: "2026-11-10T10:00:00Z" }),
      ],
      now,
    );
    expect(groups.map((g) => g.title)).toEqual(["This week", "Last week", "Week of 28 Sept"]);
    expect(groups[0]!.range).toBe("9 to 15 Nov");
    expect(groups[1]!.items[0]!.id).toBe("taken");
    expect(groups[2]!.range).toBe("28 Sept to 4 Oct");
  });
});
