import {
  Background,
  BackgroundVariant,
  MarkerType,
  MiniMap,
  Panel,
  ReactFlow,
  ReactFlowProvider,
  useNodesState,
  useReactFlow,
  useViewport,
  type Connection,
  type EdgeChange,
  type XYPosition,
  type FinalConnectionState,
  type NodeChange,
  type OnBeforeDelete,
} from "@xyflow/react";
import "@xyflow/react/dist/base.css";
import { useCallback, useEffect, useMemo, useRef, useState, type MouseEvent as ReactMouseEvent } from "react";
import { useNavigate } from "react-router";
import type { CanvasView } from "../api/client";
import { GroupNode, type GroupNodeData, type GroupNodeType } from "../canvas/GroupNode";
import { between, contents, MIN_GROUP, wrap, type Placed, type Rect } from "../canvas/groups";
import { CARD_WIDTH, LockedNode, ProjectNode, type CanvasNodeType as CardNodeType } from "../canvas/ProjectNode";
import { LinkEdge, type LinkEdgeType } from "../canvas/LinkEdge";
import { NewProjectDialog } from "../canvas/NewProjectDialog";
import { ProjectDrawer } from "../canvas/ProjectDrawer";
import {
  useCanvas,
  useCreateGroup,
  useCreateLink,
  useDeleteGroup,
  useDeleteLink,
  useSaveNodes,
  useUpdateGroup,
} from "../canvas/queries";
import { useCanvasUi } from "../canvas/store";
import { Modal } from "../components/Modal";
import { Button, ErrorNote } from "../components/ui";
import { projectColor } from "../theme/resolve";

type CanvasNodeType = CardNodeType | GroupNodeType;
type GroupActions = (id: string) => Omit<GroupNodeData, "label" | "color">;

const nodeTypes = { project: ProjectNode, locked: LockedNode, canvasGroup: GroupNode };
const CARD_HEIGHT = 140;
const edgeTypes = { link: LinkEdge };

export function CanvasPage() {
  return (
    <ReactFlowProvider>
      <CanvasBoard />
    </ReactFlowProvider>
  );
}

/** Where a node sits and how big it is, for working out what a group holds. */
function placed(n: CanvasNodeType): Placed {
  const isGroup = n.type === "canvasGroup";
  return {
    id: n.id,
    isGroup,
    x: n.position.x,
    y: n.position.y,
    width: n.measured?.width ?? n.width ?? (isGroup ? MIN_GROUP : CARD_WIDTH),
    height: n.measured?.height ?? n.height ?? (isGroup ? MIN_GROUP : CARD_HEIGHT),
  };
}

function toNodes(view: CanvasView, previous: CanvasNodeType[], actions: GroupActions): CanvasNodeType[] {
  const prev = new Map(previous.map((n) => [n.id, n]));
  // Groups sit behind the cards and links, biggest first so a group drawn
  // inside another stays on top of it.
  const groups = view.nodes
    .filter((n) => n.kind === "group")
    .sort((a, b) => (b.width ?? 0) * (b.height ?? 0) - (a.width ?? 0) * (a.height ?? 0))
    .map((n): GroupNodeType => {
      const old = prev.get(n.id);
      const keep = old?.dragging;
      return {
        id: n.id,
        type: "canvasGroup",
        position: keep ? old.position : { x: n.x, y: n.y },
        width: keep ? old.width : (n.width ?? 480),
        height: keep ? old.height : (n.height ?? 320),
        zIndex: -1,
        dragHandle: ".tmp-group-handle",
        className: "tmp-group-node",
        selected: old?.selected ?? false,
        data: { label: n.label ?? "Group", color: n.color ?? "slot-1", ...actions(n.id) },
      };
    });
  const byId = new Map(view.projects.map((p) => [p.id, p]));
  const lockedById = new Map(view.locked.map((l) => [l.id, l]));
  const parentOf = new Map<string, string>();
  for (const l of view.links) {
    if (l.kind === "spawned" && !parentOf.has(l.target_id)) parentOf.set(l.target_id, byId.get(l.source_id)?.title ?? "");
  }
  const cards = view.nodes.flatMap((n): CardNodeType[] => {
    const project = n.project_id ? byId.get(n.project_id) : undefined;
    const locked = n.project_id ? lockedById.get(n.project_id) : undefined;
    if (locked) {
      const old = prev.get(locked.id);
      return [
        {
          id: locked.id,
          type: "locked" as const,
          position: old?.dragging ? old.position : { x: n.x, y: n.y },
          selectable: false,
          data: { ownerName: locked.owner_name },
        },
      ];
    }
    if (!project) return [];
    const old = prev.get(project.id);
    return [
      {
        id: project.id,
        type: "project" as const,
        // Keep the local position mid-drag; the server's otherwise.
        position: old?.dragging ? old.position : { x: n.x, y: n.y },
        selected: old?.selected ?? false,
        data: { project, parentTitle: parentOf.get(project.id) },
      },
    ];
  });
  return [...groups, ...cards];
}

function toEdges(view: CanvasView, selectedId: string | null): LinkEdgeType[] {
  const byId = new Map(view.projects.map((p) => [p.id, p]));
  return view.links.map((link) => {
    const source = byId.get(link.source_id);
    const color = source ? projectColor(source.color) : "var(--tmp-text-quiet)";
    const markerColor = link.kind === "spawned" ? color : link.kind === "blocks" ? "var(--tmp-late)" : undefined;
    return {
      id: link.id,
      source: link.source_id,
      target: link.target_id,
      type: "link" as const,
      selected: link.id === selectedId,
      // Only links between two projects you can edit can be removed.
      deletable: [link.source_id, link.target_id].every((id) => {
        const role = byId.get(id)?.role;
        return role === "owner" || role === "editor";
      }),
      data: { link, color },
      markerEnd: markerColor ? { type: MarkerType.ArrowClosed, color: markerColor, width: 14, height: 14 } : undefined,
    };
  });
}

function CanvasBoard() {
  const showArchived = useCanvasUi((s) => s.showArchived);
  const setShowArchived = useCanvasUi((s) => s.setShowArchived);
  const openProjectId = useCanvasUi((s) => s.openProjectId);
  const openProject = useCanvasUi((s) => s.openProject);
  const draft = useCanvasUi((s) => s.newProject);
  const startNewProject = useCanvasUi((s) => s.startNewProject);
  const drawingGroup = useCanvasUi((s) => s.drawingGroup);
  const setDrawingGroup = useCanvasUi((s) => s.setDrawingGroup);
  const editGroup = useCanvasUi((s) => s.editGroup);
  const createGroup = useCreateGroup();
  const updateGroup = useUpdateGroup();
  const deleteGroup = useDeleteGroup();
  const canvas = useCanvas(showArchived);
  const saveNodes = useSaveNodes();
  const createLink = useCreateLink();
  const flow = useReactFlow<CanvasNodeType, LinkEdgeType>();
  const navigate = useNavigate();
  const [nodes, setNodes, onNodesChange] = useNodesState<CanvasNodeType>([]);
  const deleteLink = useDeleteLink();
  const [selectedEdge, setSelectedEdge] = useState<string | null>(null);
  const fitted = useRef(false);
  const mainRef = useRef<HTMLElement>(null);

  const view = canvas.data;
  const canvasId = view?.canvas.id;
  // Mutations change identity each render; keep the latest in a ref so the
  // group callbacks handed to nodes stay stable.
  const groupMutations = useRef({ saveNodes, updateGroup, deleteGroup });
  groupMutations.current = { saveNodes, updateGroup, deleteGroup };
  const groupActions = useCallback<GroupActions>(
    (groupId) => ({
      onRename: (label) => canvasId && groupMutations.current.updateGroup.mutate({ canvasId, groupId, label }),
      onRecolor: (color) => canvasId && groupMutations.current.updateGroup.mutate({ canvasId, groupId, color }),
      onDelete: () => canvasId && groupMutations.current.deleteGroup.mutate({ canvasId, groupId }),
      onResized: (r) => canvasId && groupMutations.current.saveNodes.mutate({ canvasId, nodes: [], groups: [{ id: groupId, ...r }] }),
    }),
    [canvasId],
  );
  useEffect(() => {
    if (view) setNodes((prev) => toNodes(view, prev, groupActions));
  }, [view, setNodes, groupActions]);
  // Link errors (a loop, a duplicate) show briefly, then clear.
  const { error: linkError, reset: resetLinkError } = createLink;
  useEffect(() => {
    if (!linkError) return;
    const t = setTimeout(resetLinkError, 5000);
    return () => clearTimeout(t);
  }, [linkError, resetLinkError]);

  const edges = useMemo(() => (view ? toEdges(view, selectedEdge) : []), [view, selectedEdge]);

  // Edges are derived from the server data; only selection and removal are local.
  const onEdgesChange = useCallback(
    (changes: EdgeChange<LinkEdgeType>[]) => {
      for (const c of changes) {
        if (c.type === "select") setSelectedEdge((cur) => (c.selected ? c.id : cur === c.id ? null : cur));
        if (c.type === "remove") deleteLink.mutate(c.id);
      }
    },
    [deleteLink],
  );

  // Fit the view once, the first time cards arrive.
  useEffect(() => {
    if (!fitted.current && nodes.length > 0 && nodes.every((n) => n.measured)) {
      fitted.current = true;
      void flow.fitView({ padding: 0.2, maxZoom: 1 });
    }
  }, [nodes, flow]);

  // Highlight the open project's card.
  useEffect(() => {
    setNodes((ns) => ns.map((n) => (n.selected === (n.id === openProjectId) ? n : { ...n, selected: n.id === openProjectId })));
  }, [openProjectId, setNodes]);

  const handleNodesChange = useCallback(
    (changes: NodeChange<CanvasNodeType>[]) => onNodesChange(changes.filter((c) => c.type !== "remove")),
    [onNodesChange],
  );

  // Dragging a group carries what is inside it. What it holds is worked out
  // when the drag starts, so cards it passes over on the way are left alone.
  const carry = useRef<{ from: XYPosition; anchor: string; start: Map<string, XYPosition> } | null>(null);

  const onNodeDragStart = useCallback(
    (_: MouseEvent | TouchEvent, node: CanvasNodeType, dragged: CanvasNodeType[]) => {
      const draggedIds = new Set(dragged.map((n) => n.id));
      const groups = dragged.filter((n) => n.type === "canvasGroup");
      if (groups.length === 0) {
        carry.current = null;
        return;
      }
      const all = flow.getNodes().map(placed);
      const start = new Map<string, XYPosition>();
      for (const g of groups) {
        for (const id of contents(placed(g), all)) {
          const n = flow.getNode(id);
          if (n && !draggedIds.has(id)) start.set(id, { ...n.position });
        }
      }
      carry.current = { from: { ...node.position }, anchor: node.id, start };
    },
    [flow],
  );

  const onNodeDrag = useCallback(
    (_: MouseEvent | TouchEvent, node: CanvasNodeType) => {
      const c = carry.current;
      if (!c || c.start.size === 0 || node.id !== c.anchor) return;
      const dx = node.position.x - c.from.x;
      const dy = node.position.y - c.from.y;
      setNodes((ns) =>
        ns.map((n) => {
          const s = c.start.get(n.id);
          return s ? { ...n, position: { x: s.x + dx, y: s.y + dy } } : n;
        }),
      );
    },
    [setNodes],
  );

  const onNodeDragStop = useCallback(
    (_: MouseEvent | TouchEvent, _node: CanvasNodeType, dragged: CanvasNodeType[]) => {
      if (!view) return;
      const carried = carry.current ? [...carry.current.start.keys()].map((id) => flow.getNode(id)).filter((n) => !!n) : [];
      carry.current = null;
      const moved = [...dragged, ...carried];
      const at = (n: CanvasNodeType) => ({ x: Math.round(n.position.x), y: Math.round(n.position.y) });
      saveNodes.mutate({
        canvasId: view.canvas.id,
        nodes: moved.filter((n) => n.type !== "canvasGroup").map((n) => ({ project_id: n.id, ...at(n) })),
        groups: moved.filter((n) => n.type === "canvasGroup").map((n) => ({ id: n.id, ...at(n) })),
      });
    },
    [view, saveNodes, flow],
  );

  const makeGroup = useCallback(
    (rect: Rect) => {
      if (!canvasId) return;
      const color = `slot-${((view?.nodes.filter((n) => n.kind === "group").length ?? 0) % 8) + 1}`;
      createGroup.mutate(
        { canvasId, label: "New group", color, ...rect },
        {
          onSuccess: (g) => {
            setNodes((ns) => ns.map((n) => (n.selected ? { ...n, selected: false } : n)));
            // Zoomed far out (as on a phone), bring the new group up close
            // enough to read its name while you type it.
            const box = mainRef.current;
            if (box && flow.getZoom() < 0.6) {
              const fit = Math.min(box.clientWidth / rect.width, box.clientHeight / rect.height) * 0.85;
              const zoom = Math.max(flow.getZoom(), Math.min(fit, 0.9));
              void flow.setCenter(rect.x + rect.width / 2, rect.y + rect.height / 2, { zoom, duration: 300 });
            }
            editGroup(g.id);
          },
        },
      );
    },
    [canvasId, view, createGroup, editGroup, setNodes, flow],
  );

  // "+ Group": wraps the cards you have selected (two or more), or lets you
  // draw the area yourself.
  const onGroupButton = () => {
    const picked = nodes.filter((n) => n.selected && n.type !== "canvasGroup");
    if (picked.length >= 2) {
      openProject(null);
      makeGroup(wrap(picked.map(placed)));
    } else {
      if (!drawingGroup) setNodes((ns) => ns.map((n) => (n.selected ? { ...n, selected: false } : n)));
      setDrawingGroup(!drawingGroup);
    }
  };

  const onConnect = useCallback(
    (c: Connection) => createLink.mutate({ source_id: c.source, target_id: c.target, kind: "spawned" }),
    [createLink],
  );

  // Dropping a new link on empty canvas spawns a child project there.
  const onConnectEnd = useCallback(
    (event: MouseEvent | TouchEvent, state: FinalConnectionState) => {
      if (state.isValid || state.toNode || !state.fromNode) return;
      const point = "changedTouches" in event ? event.changedTouches[0]! : event;
      const pos = flow.screenToFlowPosition({ x: point.clientX, y: point.clientY });
      startNewProject({ spawnedFrom: state.fromNode.id, position: { x: Math.round(pos.x), y: Math.round(pos.y - 40) } });
    },
    [flow, startNewProject],
  );

  // Links can be removed with the keyboard; cards cannot (use the drawer).
  const onBeforeDelete: OnBeforeDelete<CanvasNodeType, LinkEdgeType> = useCallback(async ({ edges }) => ({ nodes: [], edges }), []);

  const jumpTo = useCallback(
    (id: string) => {
      const node = flow.getNode(id);
      if (node) void flow.setCenter(node.position.x + CARD_WIDTH / 2, node.position.y + 90, { zoom: Math.max(flow.getZoom(), 0.8), duration: 400 });
      openProject(id);
    },
    [flow, openProject],
  );

  const onPaneDoubleClick = (e: ReactMouseEvent) => {
    if (!(e.target as HTMLElement).classList.contains("react-flow__pane")) return;
    const pos = flow.screenToFlowPosition({ x: e.clientX, y: e.clientY });
    startNewProject({ position: { x: Math.round(pos.x - CARD_WIDTH / 2), y: Math.round(pos.y - 60) } });
  };

  const projects = view?.projects ?? [];
  const parentTitle = draft?.spawnedFrom ? projects.find((p) => p.id === draft.spawnedFrom)?.title : undefined;
  const suggestedColor = `slot-${(projects.length % 8) + 1}`;
  const sharers = [...new Map(projects.filter((p) => p.role !== "owner").map((p) => [p.owner_id, p.owner_name])).entries()];
  const [copyFrom, setCopyFrom] = useState<{ id: string; name: string } | null>(null);

  return (
    <main ref={mainRef} className="relative flex-1" onDoubleClick={onPaneDoubleClick}>
      <ReactFlow<CanvasNodeType, LinkEdgeType>
        nodes={nodes}
        edges={edges}
        nodeTypes={nodeTypes}
        edgeTypes={edgeTypes}
        onNodesChange={handleNodesChange}
        onEdgesChange={onEdgesChange}
        onNodeDragStart={onNodeDragStart}
        onNodeDrag={onNodeDrag}
        onNodeDragStop={onNodeDragStop}
        elevateNodesOnSelect={false}
        onNodeClick={(_, n) => n.type === "project" && openProject(n.id)}
        onNodeDoubleClick={(_, n) => n.type === "project" && navigate(`/projects/${n.id}`)}
        onPaneClick={() => openProject(null)}
        onConnect={onConnect}
        onConnectEnd={onConnectEnd}
        onBeforeDelete={onBeforeDelete}
        zoomOnDoubleClick={false}
        minZoom={0.15}
        maxZoom={2}
        proOptions={{ hideAttribution: true }}
        className="tmp-flow"
      >
        <Background id="minor" variant={BackgroundVariant.Lines} gap={16} lineWidth={1} color="var(--tmp-canvas-grid-minor)" />
        <Background id="major" variant={BackgroundVariant.Lines} gap={128} lineWidth={1} color="var(--tmp-canvas-grid-major)" />

        <Panel position="top-left" className="!m-4 flex flex-wrap gap-2">
          <button
            type="button"
            aria-pressed={drawingGroup}
            onClick={onGroupButton}
            title="Draw a labelled area around projects. Select two or more cards first (Shift-drag) to wrap them."
            className={`border-rule shadow-card label-mono flex min-h-11 cursor-pointer items-center gap-2 rounded-lg px-3 ${
              drawingGroup ? "bg-accent text-on-accent" : "bg-surface-card text-text-quiet hover:text-text"
            }`}
          >
            <svg width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
              <rect x="1.5" y="3.5" width="13" height="11" rx="1.5" stroke="currentColor" strokeWidth="1.5" strokeDasharray="3 2" />
              <rect x="1.5" y="1" width="6" height="4" rx="1" fill="currentColor" />
            </svg>
            {drawingGroup ? "Cancel" : "Group"}
          </button>
          <label className="border-rule bg-surface-card shadow-card label-mono text-text-quiet hover:text-text flex min-h-11 cursor-pointer items-center gap-2 rounded-lg px-3">
            <input type="checkbox" className="accent-[var(--tmp-accent)]" checked={showArchived} onChange={(e) => setShowArchived(e.target.checked)} />
            Show archived
          </label>
          {sharers.map(([id, name]) => (
            <button
              key={id}
              type="button"
              onClick={() => setCopyFrom({ id, name })}
              title={`Move ${name}'s projects to where ${name} has them, inside copies of ${name}'s groups`}
              className="border-rule bg-surface-card shadow-card label-mono text-text-quiet hover:text-text flex min-h-11 cursor-pointer items-center gap-2 rounded-lg px-3"
            >
              <svg width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
                <rect x="1.5" y="1.5" width="8" height="8" rx="1.5" stroke="currentColor" strokeWidth="1.5" />
                <rect x="6.5" y="6.5" width="8" height="8" rx="1.5" fill="currentColor" />
              </svg>
              Copy {name}'s layout
            </button>
          ))}
        </Panel>

        <Panel position="bottom-left" className="!m-4 hidden sm:block">
          <Legend />
        </Panel>

        <Panel position="bottom-right" className="!m-4 flex items-end gap-2.5">
          <ZoomControls />
          <MiniMap
            pannable
            zoomable
            ariaLabel="Minimap"
            className="!static !m-0 hidden sm:block"
            nodeColor={(n) => {
              const node = n as CanvasNodeType;
              if (node.type === "canvasGroup") return `color-mix(in srgb, ${projectColor(node.data.color)} 25%, transparent)`;
              return node.type === "project" ? projectColor(node.data.project.color) : "var(--tmp-line)";
            }}
            nodeBorderRadius={2}
            maskColor="color-mix(in srgb, var(--tmp-surface) 70%, transparent)"
            style={{ width: 180, height: 110 }}
          />
        </Panel>
      </ReactFlow>

      {drawingGroup && <DrawLayer onDone={(r) => { setDrawingGroup(false); if (r) makeGroup(r); }} />}
      {canvas.isError && (
        <div className="absolute inset-x-4 top-20 mx-auto max-w-md"><ErrorNote error={canvas.error} /></div>
      )}
      {view && projects.length === 0 && <EmptyState onCreate={() => startNewProject()} />}
      {createLink.error && (
        <div className="absolute inset-x-4 top-20 mx-auto max-w-md"><ErrorNote error={createLink.error} /></div>
      )}

      {copyFrom && (
        <Modal title={`Copy ${copyFrom.name}'s layout`} onClose={() => setCopyFrom(null)}>
          <div className="flex flex-col gap-4">
            <p className="text-sm leading-relaxed">
              {copyFrom.name}'s {projects.filter((p) => p.owner_id === copyFrom.id).length} projects move to where{" "}
              {copyFrom.name} has them, and {copyFrom.name}'s groups are copied around them. Your own projects and groups
              stay where they are.
            </p>
            <p className="text-text-quiet text-xs leading-relaxed">
              The copies are yours: move or rename them freely. Nothing changes on {copyFrom.name}'s canvas, and later
              changes there don't follow. You can undo this straight after.
            </p>
            <div className="flex flex-wrap justify-end gap-2">
              <Button type="button" onClick={() => setCopyFrom(null)}>Cancel</Button>
              <Button type="button" variant="primary" onClick={() => setCopyFrom(null)}>Copy layout</Button>
            </div>
          </div>
        </Modal>
      )}

      <ProjectDrawer onJump={jumpTo} />
      <NewProjectDialog
        parentTitle={parentTitle}
        suggestedColor={suggestedColor}
        projects={projects
          .filter((p) => p.role !== "viewer" && p.status !== "archived")
          .map((p) => ({ id: p.id, title: p.title }))
          .sort((a, b) => a.title.localeCompare(b.title))}
      />
    </main>
  );
}

/**
 * Over the canvas while drawing a group: drag out the area, or tap for a
 * standard-sized one. Escape or the Cancel button stops.
 */
function DrawLayer({ onDone }: { onDone: (rect: Rect | null) => void }) {
  const flow = useReactFlow();
  const [start, setStart] = useState<XYPosition | null>(null);
  const [end, setEnd] = useState<XYPosition | null>(null);
  const layer = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onDone(null);
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onDone]);
  const local = (e: { clientX: number; clientY: number }) => {
    const box = layer.current!.getBoundingClientRect();
    return { x: e.clientX - box.left, y: e.clientY - box.top };
  };
  const finish = (e: React.PointerEvent) => {
    if (!start) return;
    const a = flow.screenToFlowPosition({ x: start.x + layer.current!.getBoundingClientRect().left, y: start.y + layer.current!.getBoundingClientRect().top });
    const b = flow.screenToFlowPosition({ x: e.clientX, y: e.clientY });
    let rect = between(a, b);
    if (rect.width < MIN_GROUP || rect.height < MIN_GROUP) rect = { x: Math.round(b.x - 280), y: Math.round(b.y - 40), width: 560, height: 360 };
    setStart(null);
    setEnd(null);
    onDone(rect);
  };
  const box = start && end ? { left: Math.min(start.x, end.x), top: Math.min(start.y, end.y), width: Math.abs(start.x - end.x), height: Math.abs(start.y - end.y) } : null;
  return (
    <div
      ref={layer}
      className="tmp-draw-layer absolute inset-0 z-10"
      onPointerDown={(e) => {
        e.currentTarget.setPointerCapture(e.pointerId);
        setStart(local(e));
        setEnd(local(e));
      }}
      onPointerMove={(e) => start && setEnd(local(e))}
      onPointerUp={finish}
    >
      {box && <div className="border-accent pointer-events-none absolute rounded-lg border-2 border-dashed" style={{ ...box, background: "color-mix(in srgb, var(--tmp-accent) 8%, transparent)" }} />}
      <div className="pointer-events-none absolute inset-x-4 bottom-20 flex justify-center sm:bottom-6">
        <span className="border-rule bg-surface-card shadow-card label-mono text-text rounded-lg px-3.5 py-2.5">
          Drag to draw a group · tap for a standard one · Esc to stop
        </span>
      </div>
    </div>
  );
}

function ZoomControls() {
  const flow = useReactFlow();
  const { zoom } = useViewport();
  const btn = "text-text hover:bg-surface-raised grid size-11 cursor-pointer place-items-center text-lg";
  return (
    <div className="border-rule bg-surface-card shadow-card label-mono flex flex-col overflow-hidden rounded-lg">
      <button type="button" aria-label="Zoom in" className={btn} onClick={() => flow.zoomIn({ duration: 150 })}>+</button>
      <button
        type="button"
        aria-label="Fit all cards"
        title="Fit all cards"
        className="border-rule text-text-quiet hover:text-text cursor-pointer border-x-0 py-1 text-center"
        onClick={() => flow.fitView({ padding: 0.2, maxZoom: 1, duration: 300 })}
      >
        {Math.round(zoom * 100)}%
      </button>
      <button type="button" aria-label="Zoom out" className={btn} onClick={() => flow.zoomOut({ duration: 150 })}>−</button>
    </div>
  );
}

function Legend() {
  return (
    <div className="border-rule bg-surface-card label-mono text-text-quiet flex flex-wrap gap-4 rounded-lg px-3.5 py-2.5">
      <span className="flex items-center gap-2">
        <svg width="30" height="8" aria-hidden="true"><path d="M0 4h24" stroke="var(--tmp-text)" strokeWidth="3" /><path d="M23 0l7 4-7 4z" fill="var(--tmp-text)" /></svg>
        Spawned
      </span>
      <span className="flex items-center gap-2">
        <svg width="30" height="8" aria-hidden="true"><path d="M0 4h30" stroke="var(--tmp-text-quiet)" strokeWidth="2" strokeDasharray="6 4" /></svg>
        Related
      </span>
      <span className="flex items-center gap-2">
        <svg width="30" height="8" aria-hidden="true"><path d="M0 4h24" stroke="var(--tmp-late)" strokeWidth="2" strokeDasharray="6 4" /><path d="M23 0l7 4-7 4z" fill="var(--tmp-late)" /></svg>
        Blocks
      </span>
      <span className="flex items-center gap-2">
        <svg width="22" height="14" aria-hidden="true"><rect x="1" y="1" width="20" height="12" rx="2" fill="none" stroke="var(--tmp-text-quiet)" strokeWidth="1.5" /><rect x="1" y="1" width="9" height="4" fill="var(--tmp-text-quiet)" /></svg>
        Group
      </span>
    </div>
  );
}

function EmptyState({ onCreate }: { onCreate: () => void }) {
  return (
    <div className="pointer-events-none absolute inset-0 flex items-center justify-center p-4">
      <div className="border-rule bg-surface-card shadow-card pointer-events-auto w-full max-w-[320px] rounded-lg border-t-[6px] border-t-accent p-4">
        <span className="label-mono text-accent">Empty canvas</span>
        <h1 className="font-display mt-2 text-[22px] leading-[1.05] font-extrabold tracking-[-0.02em]">Add your first project</h1>
        <p className="text-text-quiet mt-2 mb-4 text-sm leading-relaxed">
          Projects sit here as cards. Drag from a card's edge to another to link them, or into empty space to spawn a child.
          Double-click the canvas to add one anywhere.
        </p>
        <Button variant="primary" onClick={onCreate}>+ New project</Button>
      </div>
    </div>
  );
}
