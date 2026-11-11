import { useEffect, type ReactNode } from "react";
import { createBrowserRouter, Navigate, RouterProvider, useLocation } from "react-router";
import { MeContext, useAuthStatus } from "./auth/queries";
import { AppShell } from "./components/AppShell";
import { AuthLayout } from "./components/AuthLayout";
import { Button } from "./components/ui";
import { CalendarPage } from "./pages/CalendarPage";
import { CanvasPage } from "./pages/CanvasPage";
import { ConnectPage } from "./pages/ConnectPage";
import { GlobalBoardPage } from "./pages/GlobalBoardPage";
import { ProjectBoardPage } from "./pages/ProjectBoardPage";
import { ProjectDataPage } from "./pages/ProjectDataPage";
import { ProjectOverview } from "./pages/ProjectOverview";
import { ProjectPage } from "./pages/ProjectPage";
import { ProjectPhotosPage } from "./pages/ProjectPhotosPage";
import { InvitePage } from "./pages/InvitePage";
import { LoginPage } from "./pages/LoginPage";
import { NotesIndex, NotesLayout } from "./pages/NotesPage";
import { ProjectNotesPage } from "./pages/ProjectNotesPage";
import { SettingsPage } from "./pages/SettingsPage";
import { ShoppingPage } from "./pages/ShoppingPage";
import { SetupPage } from "./pages/SetupPage";
import { TodayPage } from "./pages/TodayPage";
import { ThemeSync } from "./theme/ThemeSync";

/** Sends people to setup, login or the app, depending on who they are. */
function Gate({ need, children }: { need: "setup" | "guest" | "user"; children: ReactNode }) {
  const status = useAuthStatus();
  const location = useLocation();
  if (status.isPending) return null;
  if (status.isError)
    return (
      <AuthLayout title="Can't reach the server" intro={status.error.message}>
        <Button onClick={() => status.refetch()}>Try again</Button>
      </AuthLayout>
    );
  const { needs_setup, user } = status.data;
  if (needs_setup && need !== "setup") return <Navigate to="/setup" replace />;
  if (!needs_setup && need === "setup") return <Navigate to={user ? "/" : "/login"} replace />;
  if (need === "user" && !user) return <Navigate to="/login" replace state={{ from: location.pathname + location.search }} />;
  // Signing in lands here first; carry on to where the person was going (such as the Allow page).
  if (need === "guest" && user)
    return <Navigate to={(location.state as { from?: string } | null)?.from ?? "/"} replace />;
  return <MeContext.Provider value={user ?? null}>{children}</MeContext.Provider>;
}

/** The site opens on Today; after that, the Canvas link is the canvas. */
// Opened straight at an app page (a link, a reload): leave it be. Signing in,
// setting up or accepting an invite still ends up on Today.
const path = window.location.pathname;
let landed = !(path === "/" || path === "/login" || path === "/setup" || path.startsWith("/invite/"));
function Home() {
  const toToday = !landed;
  useEffect(() => {
    landed = true;
  }, []);
  if (toToday) return <Navigate to={{ pathname: "/today", search: window.location.search }} replace />;
  return <CanvasPage />;
}

const router = createBrowserRouter([
  { path: "/setup", element: <Gate need="setup"><SetupPage /></Gate> },
  { path: "/login", element: <Gate need="guest"><LoginPage /></Gate> },
  { path: "/invite/:token", element: <Gate need="guest"><InvitePage /></Gate> },
  // The Allow page an app such as Claude opens when it connects by signing in.
  { path: "/connect", element: <Gate need="user"><ConnectPage /></Gate> },
  {
    element: <Gate need="user"><AppShell /></Gate>,
    children: [
      { index: true, element: <Home /> },
      { path: "today", element: <TodayPage /> },
      // A share sent without the service worker (it wasn't installed yet): Today handles ?add=1.
      { path: "share", element: <Navigate to="/today?add=1" replace /> },
      { path: "shopping", element: <ShoppingPage /> },
      { path: "board", element: <GlobalBoardPage /> },
      { path: "calendar", element: <CalendarPage /> },
      {
        path: "projects/:id",
        element: <ProjectPage />,
        children: [
          { index: true, element: <ProjectOverview /> },
          { path: "board", element: <ProjectBoardPage /> },
          { path: "photos", element: <ProjectPhotosPage /> },
          { path: "notes", element: <ProjectNotesPage /> },
          { path: "data", element: <ProjectDataPage /> },
          { path: "data/:dbId", element: <ProjectDataPage /> },
        ],
      },
      {
        path: "notes",
        element: <NotesLayout />,
        children: [
          { index: true, element: <NotesIndex /> },
          // The editor and markdown renderer load only when a note opens.
          { path: ":id", lazy: async () => ({ Component: (await import("./pages/NotePage")).NotePage }) },
        ],
      },
      { path: "settings", element: <SettingsPage /> },
    ],
  },
  { path: "*", element: <Navigate to="/" replace /> },
]);

export function App() {
  const status = useAuthStatus();
  const user = status.data?.user;
  return (
    <>
      <ThemeSync themeId={user?.theme_id} mode={user?.theme_mode} />
      <RouterProvider router={router} />
    </>
  );
}
