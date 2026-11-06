/** Up to two capital letters for an avatar: "Sam Lee" → "SL". */
export function initials(name: string): string {
  const words = name.trim().split(/\s+/).filter(Boolean);
  const letters = words.length > 1 ? [words[0]![0], words[words.length - 1]![0]] : [...(words[0] ?? "?")].slice(0, 2);
  return letters.join("").toUpperCase();
}

export const ROLE_LABEL: Record<string, string> = { owner: "Owner", editor: "Editor", viewer: "Viewer" };

/** How a project shared with you reads in a badge, or null for your own. */
export function sharedLabel(p: { role: string; owner_name: string }): string | null {
  if (p.role === "owner") return null;
  return `${p.owner_name}’s · ${p.role === "editor" ? "can edit" : "view only"}`;
}
