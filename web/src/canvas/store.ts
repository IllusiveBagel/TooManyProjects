import { create } from "zustand";

export interface NewProjectDraft {
  /** Where the card goes, in canvas coordinates. */
  position?: { x: number; y: number };
  /** Parent project, when spawned by dragging a link into empty space. */
  spawnedFrom?: string;
}

interface CanvasUiState {
  /** Project open in the side drawer. */
  openProjectId: string | null;
  /** The New project dialog, when open. */
  newProject: NewProjectDraft | null;
  showArchived: boolean;
  /** Drawing a new group: the next drag on the canvas draws its area. */
  drawingGroup: boolean;
  /** The group whose name is being typed. */
  editingGroupId: string | null;
  openProject: (id: string | null) => void;
  startNewProject: (draft?: NewProjectDraft) => void;
  closeNewProject: () => void;
  setShowArchived: (show: boolean) => void;
  setDrawingGroup: (on: boolean) => void;
  editGroup: (id: string | null) => void;
}

export const useCanvasUi = create<CanvasUiState>((set) => ({
  openProjectId: null,
  newProject: null,
  showArchived: false,
  drawingGroup: false,
  editingGroupId: null,
  openProject: (id) => set({ openProjectId: id }),
  startNewProject: (draft = {}) => set({ newProject: draft }),
  closeNewProject: () => set({ newProject: null }),
  setShowArchived: (showArchived) => set({ showArchived }),
  setDrawingGroup: (drawingGroup) => set({ drawingGroup }),
  editGroup: (editingGroupId) => set({ editingGroupId }),
}));
