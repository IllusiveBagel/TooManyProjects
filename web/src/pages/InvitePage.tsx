import { useQuery } from "@tanstack/react-query";
import { useState, type FormEvent } from "react";
import { Link, useNavigate, useParams } from "react-router";
import { api, unwrap } from "../api/client";
import { AuthLayout } from "../components/AuthLayout";
import { Button, ErrorNote, Field } from "../components/ui";
import { useAcceptInvite } from "../auth/queries";

export function InvitePage() {
  const { token = "" } = useParams();
  const navigate = useNavigate();
  const accept = useAcceptInvite();
  const [form, setForm] = useState({ name: "", email: "", password: "" });
  const lookup = useQuery({
    queryKey: ["invite", token],
    queryFn: () => unwrap(api.GET("/invites/lookup/{token}", { params: { path: { token } } })),
    retry: false,
  });

  if (lookup.isPending) return <AuthLayout title="Checking your invite…">{null}</AuthLayout>;
  if (lookup.isError)
    return (
      <AuthLayout title="This invite can't be used" intro="It may have expired, been used already, or been cancelled. Ask for a new link.">
        <Link to="/login" className="text-accent underline">Go to sign in</Link>
      </AuthLayout>
    );

  const submit = (e: FormEvent) => {
    e.preventDefault();
    accept.mutate({ token, ...form }, { onSuccess: () => navigate("/", { replace: true }) });
  };

  return (
    <AuthLayout title="Join the workshop" intro={`${lookup.data.invited_by} invited you. Make your account to get in.`}>
      <form onSubmit={submit} className="flex flex-col gap-4">
        <Field label="Name" autoComplete="name" required value={form.name}
          onChange={(e) => setForm({ ...form, name: e.target.value })} />
        <Field label="Email" type="email" autoComplete="email" required value={form.email}
          onChange={(e) => setForm({ ...form, email: e.target.value })} />
        <Field label="Password" type="password" autoComplete="new-password" required minLength={10}
          hint="At least 10 characters." value={form.password}
          onChange={(e) => setForm({ ...form, password: e.target.value })} />
        <ErrorNote error={accept.error} />
        <Button variant="primary" type="submit" disabled={accept.isPending}>
          {accept.isPending ? "Creating…" : "Create account"}
        </Button>
      </form>
    </AuthLayout>
  );
}
