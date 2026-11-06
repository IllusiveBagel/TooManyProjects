import { NavLink, Outlet } from "react-router";
import { useLogout, useMe } from "../auth/queries";
import { Wordmark } from "./ui";

const navClass = ({ isActive }: { isActive: boolean }) =>
  `label-mono border-b-2 py-2 text-xs ${isActive ? "text-text border-accent" : "text-text-quiet border-transparent hover:text-text"}`;

export function AppShell() {
  const me = useMe();
  const logout = useLogout();
  return (
    <div className="flex min-h-screen flex-col">
      <header className="border-rule bg-surface flex flex-wrap items-center gap-x-6 gap-y-2 border-x-0 border-t-0 px-4 py-2.5 sm:px-5">
        <NavLink to="/" aria-label="Canvas">
          {/* Project count arrives with projects in phase 1. */}
          <Wordmark count={0} />
        </NavLink>
        <nav className="flex gap-5" aria-label="Main">
          <NavLink to="/" end className={navClass}>Canvas</NavLink>
          <NavLink to="/settings" className={navClass}>Settings</NavLink>
        </nav>
        <div className="ml-auto flex items-center gap-3">
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
      <Outlet />
    </div>
  );
}
