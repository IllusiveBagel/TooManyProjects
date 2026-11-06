import { useState, type FormEvent } from "react";
import { useNavigate } from "react-router";
import { AuthLayout } from "../components/AuthLayout";
import { Button, ErrorNote, Field } from "../components/ui";
import { useSetup } from "../auth/queries";

export function SetupPage() {
  const setup = useSetup();
  const navigate = useNavigate();
  const [form, setForm] = useState({ name: "", email: "", password: "" });

  const submit = (e: FormEvent) => {
    e.preventDefault();
    setup.mutate(form, { onSuccess: () => navigate("/", { replace: true }) });
  };

  return (
    <AuthLayout
      title="Set up your workshop"
      intro="This is a new install. The account you make now is the owner. Everyone else joins by invite link."
    >
      <form onSubmit={submit} className="flex flex-col gap-4">
        <Field label="Name" autoComplete="name" required value={form.name}
          onChange={(e) => setForm({ ...form, name: e.target.value })} />
        <Field label="Email" type="email" autoComplete="email" required value={form.email}
          onChange={(e) => setForm({ ...form, email: e.target.value })} />
        <Field label="Password" type="password" autoComplete="new-password" required minLength={10}
          hint="At least 10 characters." value={form.password}
          onChange={(e) => setForm({ ...form, password: e.target.value })} />
        <ErrorNote error={setup.error} />
        <Button variant="primary" type="submit" disabled={setup.isPending}>
          {setup.isPending ? "Creating…" : "Create owner account"}
        </Button>
      </form>
    </AuthLayout>
  );
}
