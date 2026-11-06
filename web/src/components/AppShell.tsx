import { NavLink, Outlet, useNavigate } from "react-router";
import { useLogout, useMe } from "../auth/queries";
import { useCanvas } from "../canvas/queries";
import { useCanvasUi } from "../canvas/store";
import { Button, Wordmark } from "./ui";

const navClass = ({ isActive }: { isActive: boolean }) =>
  `label-mono border-b-2 py-2 text-xs ${isActive ? "text-text border-accent" : "text-text-quiet border-transparent hover:text-text"}`;

export function AppShell() {
  const me = useMe();
  const logout = useLogout();
  const navigate = useNavigate();
  const startNewProject = useCanvasUi((s) => s.startNewProject);
  const count = useCanvas(false).data?.projects.length;
  return (
    <div className="flex h-dvh flex-col">
      <header className="border-rule bg-surface flex flex-wrap items-center gap-x-6 gap-y-2 border-x-0 border-t-0 px-4 py-2.5 sm:px-5">
        <NavLink to="/" aria-label="Canvas">
          <Wordmark count={count} />
        </NavLink>
        <nav className="flex gap-5" aria-label="Main">
          <NavLink to="/" end className={navClass}>Canvas</NavLink>
          <NavLink to="/settings" className={navClass}>Settings</NavLink>
        </nav>
        <div className="ml-auto flex items-center gap-3">
          <Button
            variant="primary"
            className="min-h-10 px-3 text-sm"
            onClick={() => {
              navigate("/");
              startNewProject();
            }}
          >
            <span className="sm:hidden">+ New</span>
            <span className="hidden sm:inline">+ New project</span>
          </Button>
          <span className="label-mono text-text-quiet hidden sm:inline">{me.name}</span>
          <button
            type="button"
            onClick={() => logout.mutate()}
            className="label-mono text-text-quiet hover:text-text min-h-11 cursor-pointer px-2"
          >
            Sign out
          </button>
        </div>
      </header>
      <div className="flex min-h-0 flex-1 flex-col overflow-auto">
        <Outlet />
      </div>
    </div>
  );
}
