import { useState, type FormEvent } from "react";
import { useLocation, useNavigate } from "react-router";
import { AuthLayout } from "../components/AuthLayout";
import { Button, ErrorNote, Field } from "../components/ui";
import { useLogin } from "../auth/queries";

export function LoginPage() {
  const login = useLogin();
  const navigate = useNavigate();
  const from = (useLocation().state as { from?: string } | null)?.from ?? "/";
  const [form, setForm] = useState({ email: "", password: "" });

  const submit = (e: FormEvent) => {
    e.preventDefault();
    login.mutate(form, { onSuccess: () => navigate(from, { replace: true }) });
  };

  return (
    <AuthLayout title="Sign in">
      <form onSubmit={submit} className="flex flex-col gap-4">
        <Field label="Email" type="email" autoComplete="email" required autoFocus value={form.email}
          onChange={(e) => setForm({ ...form, email: e.target.value })} />
        <Field label="Password" type="password" autoComplete="current-password" required value={form.password}
          onChange={(e) => setForm({ ...form, password: e.target.value })} />
        <ErrorNote error={login.error} />
        <Button variant="primary" type="submit" disabled={login.isPending}>
          {login.isPending ? "Signing in…" : "Sign in"}
        </Button>
        <p className="text-text-quiet text-center text-xs">No account? Ask the owner of this install for an invite link.</p>
      </form>
    </AuthLayout>
  );
}
