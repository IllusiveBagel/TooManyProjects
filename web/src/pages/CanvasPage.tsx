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
  type FinalConnectionState,
  type NodeChange,
  type OnBeforeDelete,
} from "@xyflow/react";
import "@xyflow/react/dist/base.css";
import { useCallback, useEffect, useMemo, useRef, useState, type MouseEvent as ReactMouseEvent } from "react";
import { useNavigate } from "react-router";
import type { CanvasView } from "../api/client";
import { CARD_WIDTH, LockedNode, ProjectNode, type CanvasNodeType } from "../canvas/ProjectNode";
import { LinkEdge, type LinkEdgeType } from "../canvas/LinkEdge";
import { NewProjectDialog } from "../canvas/NewProjectDialog";
import { ProjectDrawer } from "../canvas/ProjectDrawer";
import { useCanvas, useCreateLink, useDeleteLink, useSaveNodes } from "../canvas/queries";
import { useCanvasUi } from "../canvas/store";
import { Button, ErrorNote } from "../components/ui";
import { projectColor } from "../theme/resolve";

const nodeTypes = { project: ProjectNode, locked: LockedNode };
const edgeTypes = { link: LinkEdge };

export function CanvasPage() {
  return (
    <ReactFlowProvider>
      <CanvasBoard />
    </ReactFlowProvider>
  );
}

function toNodes(view: CanvasView, previous: CanvasNodeType[]): CanvasNodeType[] {
  const prev = new Map(previous.map((n) => [n.id, n]));
  const byId = new Map(view.projects.map((p) => [p.id, p]));
  const lockedById = new Map(view.locked.map((l) => [l.id, l]));
  const parentOf = new Map<string, string>();
  for (const l of view.links) {
    if (l.kind === "spawned" && !parentOf.has(l.target_id)) parentOf.set(l.target_id, byId.get(l.source_id)?.title ?? "");
  }
  return view.nodes.flatMap((n): CanvasNodeType[] => {
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
  const canvas = useCanvas(showArchived);
  const saveNodes = useSaveNodes();
  const createLink = useCreateLink();
  const flow = useReactFlow();
  const navigate = useNavigate();
  const [nodes, setNodes, onNodesChange] = useNodesState<CanvasNodeType>([]);
  const deleteLink = useDeleteLink();
  const [selectedEdge, setSelectedEdge] = useState<string | null>(null);
  const fitted = useRef(false);

  const view = canvas.data;
  useEffect(() => {
    if (view) setNodes((prev) => toNodes(view, prev));
  }, [view, setNodes]);
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

  const onNodeDragStop = useCallback(
    (_: MouseEvent | TouchEvent, _node: CanvasNodeType, dragged: CanvasNodeType[]) => {
      if (!view) return;
      saveNodes.mutate({
        canvasId: view.canvas.id,
        nodes: dragged.map((n) => ({ project_id: n.id, x: Math.round(n.position.x), y: Math.round(n.position.y) })),
      });
    },
    [view, saveNodes],
  );

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

  return (
    <main className="relative flex-1" onDoubleClick={onPaneDoubleClick}>
      <ReactFlow<CanvasNodeType, LinkEdgeType>
        nodes={nodes}
        edges={edges}
        nodeTypes={nodeTypes}
        edgeTypes={edgeTypes}
        onNodesChange={handleNodesChange}
        onEdgesChange={onEdgesChange}
        onNodeDragStop={onNodeDragStop}
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

        <Panel position="top-left" className="!m-4">
          <label className="border-rule bg-surface-card shadow-card label-mono text-text-quiet hover:text-text flex min-h-11 cursor-pointer items-center gap-2 rounded-lg px-3">
            <input type="checkbox" className="accent-[var(--tmp-accent)]" checked={showArchived} onChange={(e) => setShowArchived(e.target.checked)} />
            Show archived
          </label>
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
              return node.type === "project" ? projectColor(node.data.project.color) : "var(--tmp-line)";
            }}
            nodeBorderRadius={2}
            maskColor="color-mix(in srgb, var(--tmp-surface) 70%, transparent)"
            style={{ width: 180, height: 110 }}
          />
        </Panel>
      </ReactFlow>

      {canvas.isError && (
        <div className="absolute inset-x-4 top-20 mx-auto max-w-md"><ErrorNote error={canvas.error} /></div>
      )}
      {view && projects.length === 0 && <EmptyState onCreate={() => startNewProject()} />}
      {createLink.error && (
        <div className="absolute inset-x-4 top-20 mx-auto max-w-md"><ErrorNote error={createLink.error} /></div>
      )}

      <ProjectDrawer onJump={jumpTo} />
      <NewProjectDialog parentTitle={parentTitle} suggestedColor={suggestedColor} />
    </main>
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
