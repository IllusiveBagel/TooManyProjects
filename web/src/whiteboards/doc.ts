// One open whiteboard: its elements, what still has to be saved, and undo.
//
// Every local change bumps the element's version and marks it dirty; the sync
// hook sends dirty elements and the server keeps the higher version. Changes
// from other people are taken in unless the element is being dragged here or
// has an unsaved change, so nothing jumps under your pointer.

import type { components } from "../api/schema";
import { fromData, sortByZ, toData, type El } from "./model";

export type ServerElement = components["schemas"]["Element"];
export type ElementChange = components["schemas"]["ElementChange"];
export type Changes = Record<string, El | null>;
export type SaveState = "saved" | "saving" | "unsaved" | "offline";

interface Entry {
  before: Changes;
  after: Changes;
}

const MAX_UNDO = 200;

export class BoardDoc {
  private els = new Map<string, El>();
  private versions = new Map<string, number>();
  /** Kept after a delete, which the server still wants a kind for. */
  private kinds = new Map<string, string>();
  /** Changed here, not sent yet. An id not in `els` is a deletion. */
  private dirty = new Set<string>();
  /** Sent, waiting for the server. */
  private sending = new Set<string>();
  /** Being dragged or typed in here: other people's changes wait. */
  private held = new Set<string>();
  private undos: Entry[] = [];
  private redos: Entry[] = [];
  private listeners = new Set<() => void>();
  private sorted: El[] = [];
  private stale = true;
  /** The last save number we have everything up to. */
  seq = 0;
  loaded = false;
  state: SaveState = "saved";
  /** Bumps on every change, for useSyncExternalStore. */
  rev = 0;

  subscribe = (f: () => void) => {
    this.listeners.add(f);
    return () => {
      this.listeners.delete(f);
    };
  };
  getRev = () => this.rev;

  private emit() {
    this.rev++;
    this.stale = true;
    for (const f of this.listeners) f();
  }

  /** Elements in drawing order. */
  list(): El[] {
    if (this.stale) {
      this.sorted = sortByZ(this.els.values());
      this.stale = false;
    }
    return this.sorted;
  }
  get(id: string) {
    return this.els.get(id);
  }
  map() {
    return this.els;
  }
  get canUndo() {
    return this.undos.length > 0;
  }
  get canRedo() {
    return this.redos.length > 0;
  }
  /** Something here hasn't reached the server yet. */
  get pending() {
    return this.dirty.size > 0 || this.sending.size > 0;
  }
  /** Top of the stack, for new elements. */
  nextZ() {
    let z = 0;
    for (const e of this.els.values()) z = Math.max(z, e.z);
    return z + 1;
  }
  minZ() {
    let z = Infinity;
    for (const e of this.els.values()) z = Math.min(z, e.z);
    return z === Infinity ? 0 : z;
  }

  /** The board as the server has it now. */
  load(elements: ServerElement[], seq: number) {
    this.els.clear();
    this.versions.clear();
    this.kinds.clear();
    for (const s of elements) {
      this.versions.set(s.id, s.version);
      const e = s.deleted ? null : fromData(s.id, s.kind, s.data as Record<string, unknown>);
      this.kinds.set(s.id, s.kind);
      if (e) this.els.set(e.id, e);
    }
    this.seq = seq;
    this.loaded = true;
    this.emit();
  }

  private put(changes: Changes) {
    for (const [id, e] of Object.entries(changes)) {
      if (e) {
        this.els.set(id, e);
        this.kinds.set(id, e.kind);
      }
      else this.els.delete(id);
    }
  }

  private mark(ids: Iterable<string>) {
    for (const id of ids) {
      this.versions.set(id, (this.versions.get(id) ?? 0) + 1);
      this.dirty.add(id);
    }
    this.state = "unsaved";
  }

  /** Shows a change without saving it or adding an undo step, as during a drag. */
  live(changes: Changes) {
    this.put(changes);
    this.emit();
  }

  /** Holds elements still while you drag or type in them. */
  hold(ids: Iterable<string>) {
    for (const id of ids) this.held.add(id);
  }
  release() {
    this.held.clear();
  }

  /** Saves a change and makes it one undo step. */
  change(changes: Changes) {
    const before: Changes = {};
    for (const id of Object.keys(changes)) before[id] = this.els.get(id) ?? null;
    this.put(changes);
    this.record(before);
  }

  /**
   * Ends a gesture: whatever changed since `before` (the elements as they were
   * when it started) is saved as one undo step.
   */
  record(before: Changes) {
    const after: Changes = {};
    const real: Changes = {};
    for (const [id, was] of Object.entries(before)) {
      const now = this.els.get(id) ?? null;
      if (now === was || (now && was && JSON.stringify(now) === JSON.stringify(was))) continue;
      after[id] = now;
      real[id] = was;
    }
    if (!Object.keys(after).length) {
      this.emit();
      return;
    }
    this.undos.push({ before: real, after });
    if (this.undos.length > MAX_UNDO) this.undos.shift();
    this.redos = [];
    this.mark(Object.keys(after));
    this.emit();
  }

  /** Puts elements back as they were before a gesture, without saving anything. */
  revert(before: Changes) {
    this.put(before);
    this.emit();
  }

  undo() {
    const e = this.undos.pop();
    if (!e) return;
    this.put(e.before);
    this.mark(Object.keys(e.before));
    this.redos.push(e);
    this.emit();
  }

  redo() {
    const e = this.redos.pop();
    if (!e) return;
    this.put(e.after);
    this.mark(Object.keys(e.after));
    this.undos.push(e);
    this.emit();
  }

  /** Takes in what other people saved. */
  applyRemote(elements: ServerElement[], seq: number) {
    let changed = false;
    for (const s of elements) {
      const mine = this.versions.get(s.id) ?? 0;
      const busy = this.dirty.has(s.id) || this.sending.has(s.id) || this.held.has(s.id);
      if (s.version < mine) continue;
      // Same version: ours, coming back, unless someone else's copy won a tie.
      if (s.version === mine && (busy || !this.differs(s))) continue;
      // Ours goes out next with a version above theirs, so the newest change wins.
      this.versions.set(s.id, s.version);
      if (busy) continue;
      this.adopt(s);
      changed = true;
    }
    this.seq = Math.max(this.seq, seq);
    if (changed) this.emit();
  }

  private adopt(s: ServerElement) {
    const e = s.deleted ? null : fromData(s.id, s.kind, s.data as Record<string, unknown>);
    this.kinds.set(s.id, s.kind);
    if (e) this.els.set(s.id, e);
    else this.els.delete(s.id);
  }

  /** Same version as ours, but someone else's copy won the tie on the server. */
  private differs(s: ServerElement) {
    const here = this.els.get(s.id);
    if (s.deleted) return !!here;
    if (!here) return true;
    return JSON.stringify(toData(here)) !== JSON.stringify(s.data);
  }

  /** What to send now. Moves it from dirty to sending. */
  takeChanges(): ElementChange[] {
    const out: ElementChange[] = [];
    for (const id of this.dirty) {
      const e = this.els.get(id);
      const version = this.versions.get(id) ?? 1;
      if (e) out.push({ id, kind: e.kind, version, data: toData(e) as ElementChange["data"] });
      else out.push({ id, kind: this.kinds.get(id) ?? "box", version, deleted: true, data: {} });
      this.sending.add(id);
    }
    this.dirty.clear();
    if (out.length) {
      this.state = "saving";
      this.emit();
    }
    return out;
  }

  /** The server took the save; `rejected` lost to newer versions and are shown instead. */
  saved(rejected: ServerElement[]) {
    this.sending.clear();
    for (const s of rejected) {
      this.versions.set(s.id, s.version);
      if (!this.dirty.has(s.id)) this.adopt(s);
    }
    this.state = this.dirty.size ? "unsaved" : "saved";
    this.emit();
  }

  /** The save didn't get through: try again with the next one. */
  failed(offline: boolean) {
    for (const id of this.sending) this.dirty.add(id);
    this.sending.clear();
    this.state = offline ? "offline" : "unsaved";
    this.emit();
  }
}
