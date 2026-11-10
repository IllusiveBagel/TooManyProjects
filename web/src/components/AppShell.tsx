import { NavLink, Outlet, useLocation, useNavigate } from "react-router";
import { useLogout, useMe } from "../auth/queries";
import { useCanvas } from "../canvas/queries";
import { useCanvasUi } from "../canvas/store";
import { QuickNoteDialog, useQuickNote, useQuickNoteShortcut } from "../notes/QuickNote";
import { UpdateBanner } from "../system/UpdateBanner";
import { Button, Wordmark } from "./ui";

const navClass = ({ isActive }: { isActive: boolean }) =>
  `label-mono border-b-2 py-2 text-xs ${isActive ? "text-text border-accent" : "text-text-quiet border-transparent hover:text-text"}`;

/** Phone tab bar: the active tab gets a bar along its top edge. */
const tabClass = ({ isActive }: { isActive: boolean }) =>
  `label-mono flex min-h-14 flex-1 flex-col items-center justify-center gap-1 border-t-2 text-[11px] ${isActive ? "text-text border-accent" : "text-text-quiet border-transparent"}`;

const TABS = [
  { to: "/", label: "Canvas", end: true, icon: "M4 4h6v6H4zM14 14h6v6h-6zM10 7h4v10" },
  { to: "/board", label: "Board", end: false, icon: "M4 4h4v16H4zM10 4h4v10h-4zM16 4h4v13h-4z" },
  { to: "/calendar", label: "Calendar", end: false, icon: "M4 6h16v14H4zM4 10h16M8 3v5M16 3v5" },
  { to: "/notes", label: "Notes", end: false, icon: "M6 3h9l4 4v14H6zM14 3v5h5M9 12h7M9 16h7" },
];
const SETTINGS_ICON = "M4 7h10M18 7h2M4 17h2M10 17h10M14 5v4M6 15v4";

export function AppShell() {
  const me = useMe();
  const logout = useLogout();
  const navigate = useNavigate();
  const { pathname } = useLocation();
  const startNewProject = useCanvasUi((s) => s.startNewProject);
  const count = useCanvas(false).data?.projects.length;
  const startQuickNote = useQuickNote((s) => s.start);
  useQuickNoteShortcut();
  // In Notes, the top bar's New button makes a note; everywhere else, a project.
  const inNotes = pathname.startsWith("/notes");
  const inCalendar = pathname.startsWith("/calendar");
  return (
    <div className="flex h-dvh flex-col">
      <header className="border-rule bg-surface flex items-center gap-x-6 border-x-0 border-t-0 px-4 py-2.5 sm:px-5">
        <NavLink to="/" aria-label="Canvas">
          <Wordmark count={count} />
        </NavLink>
        <nav className="hidden gap-5 sm:flex" aria-label="Main">
          <NavLink to="/" end className={navClass}>Canvas</NavLink>
          <NavLink to="/board" className={navClass}>Board</NavLink>
          <NavLink to="/calendar" className={navClass}>Calendar</NavLink>
          <NavLink to="/notes" className={navClass}>Notes</NavLink>
          <NavLink to="/settings" className={navClass}>Settings</NavLink>
        </nav>
        <div className="ml-auto flex items-center gap-2 sm:gap-3">
          <Button
            variant="primary"
            className="min-h-10 px-3 text-sm"
            onClick={() => {
              if (inNotes) return startQuickNote();
              if (inCalendar) return navigate("/calendar?plan=new");
              navigate("/");
              startNewProject();
            }}
          >
            <span className="sm:hidden">{inCalendar ? "+ Plan" : "+ New"}</span>
            <span className="hidden sm:inline">{inNotes ? "+ Quick note" : inCalendar ? "+ Plan time" : "+ New project"}</span>
          </Button>
          {/* Phones keep Settings up here, so the tab bar has room for Today later. */}
          <NavLink
            to="/settings"
            aria-label="Settings"
            className={({ isActive }) =>
              `grid size-10 place-items-center rounded-md sm:hidden ${isActive ? "text-text bg-surface-raised" : "text-text-quiet"}`
            }
          >
            <svg viewBox="0 0 24 24" className="size-5" fill="none" stroke="currentColor" strokeWidth="1.75" aria-hidden="true">
              <path d={SETTINGS_ICON} />
            </svg>
          </NavLink>
          <span className="label-mono text-text-quiet hidden sm:inline">{me.name}</span>
          <button
            type="button"
            onClick={() => logout.mutate()}
            className="label-mono text-text-quiet hover:text-text hidden min-h-11 cursor-pointer px-2 sm:block"
          >
            Sign out
          </button>
        </div>
      </header>
      <UpdateBanner />
      <div className="flex min-h-0 flex-1 flex-col overflow-auto">
        <Outlet />
      </div>
      <QuickNoteDialog />
      <nav
        aria-label="Main"
        className="border-rule bg-surface flex border-x-0 border-b-0 pb-[env(safe-area-inset-bottom)] sm:hidden"
      >
        {TABS.map((t) => (
          <NavLink
            key={t.to}
            to={t.to}
            end={t.end}
            className={({ isActive }) =>
              // A project's pages open from the canvas, so they keep the Canvas tab lit.
              tabClass({ isActive: isActive || (t.to === "/" && pathname.startsWith("/projects/")) })
            }
          >
            <svg viewBox="0 0 24 24" className="size-5" fill="none" stroke="currentColor" strokeWidth="1.75" aria-hidden="true">
              <path d={t.icon} />
            </svg>
            {t.label}
          </NavLink>
        ))}
      </nav>
    </div>
  );
}
