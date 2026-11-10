import { useState } from "react";
import type { ProjectTemplate } from "../api/client";
import { Button, ErrorNote, Panel } from "../components/ui";
import { bringsText } from "./format";
import {
  useCreateTemplate,
  useDeleteTemplate,
  useTemplates,
  useUpdateTemplate,
} from "./queries";

/** Settings → Templates: yours (rename, share, delete), shared ones, and copies of the built-ins. */
export function TemplatesPanel() {
  const templates = useTemplates().data ?? [];
  const copy = useCreateTemplate();
  const saved = templates.filter((t) => !t.builtin);
  const builtins = templates.filter(
    (t) => t.builtin && t.id !== "builtin-blank",
  );
  return (
    <Panel title="Templates">
      <p className="text-text-quiet text-sm">
        New projects can start from a template. Save one from a project with
        Save as template in its panel on the canvas.
      </p>
      {saved.length === 0 ? (
        <p className="label-mono text-text-quiet">
          No templates of your own yet
        </p>
      ) : (
        <ul className="flex flex-col">
          {saved.map((t) => (
            <Row key={t.id} template={t} />
          ))}
        </ul>
      )}
      <div className="flex flex-col gap-2">
        <span className="label-mono text-text-quiet">
          Copy a built-in to change it
        </span>
        <div className="flex flex-wrap gap-2">
          {builtins.map((t) => (
            <Button
              key={t.id}
              className="min-h-10 px-3 text-sm"
              disabled={copy.isPending}
              onClick={() =>
                copy.mutate({
                  name: `${t.name} (copy)`,
                  from_template_id: t.id,
                })
              }
            >
              {t.name}
            </Button>
          ))}
        </div>
        <ErrorNote error={copy.error} />
      </div>
    </Panel>
  );
}

function Row({ template: t }: { template: ProjectTemplate }) {
  const update = useUpdateTemplate();
  const remove = useDeleteTemplate();
  const [name, setName] = useState(t.name);
  const [confirm, setConfirm] = useState(false);
  return (
    <li className="border-rule flex flex-wrap items-center gap-x-3 gap-y-2 border-x-0 border-t-0 py-3 last:border-b-0">
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        {t.mine ? (
          <input
            aria-label="Template name"
            className="font-display min-w-0 bg-transparent text-[17px] font-extrabold outline-none focus:underline"
            value={name}
            maxLength={80}
            onChange={(e) => setName(e.target.value)}
            onBlur={() =>
              name.trim() &&
              name !== t.name &&
              update.mutate({ id: t.id, name })
            }
          />
        ) : (
          <span className="font-display text-[17px] font-extrabold">
            {t.name}
          </span>
        )}
        <span className="text-text-quiet text-sm">
          {bringsText(t)}
          {t.source_title && ` · from ${t.source_title}`}
          {!t.mine && ` · shared by ${t.owner_name}`}
        </span>
      </div>
      {t.mine && (
        <div className="flex items-center gap-2">
          <label className="label-mono text-text-quiet flex min-h-10 cursor-pointer items-center gap-2 text-[11px]">
            <input
              type="checkbox"
              className="accent-accent size-4"
              checked={t.shared}
              onChange={(e) =>
                update.mutate({ id: t.id, shared: e.target.checked })
              }
            />
            Shared
          </label>
          {confirm ? (
            <>
              <Button
                className="text-late min-h-10 px-3 text-sm"
                onClick={() => remove.mutate(t.id)}
              >
                Delete
              </Button>
              <Button
                className="min-h-10 px-3 text-sm"
                onClick={() => setConfirm(false)}
              >
                Keep
              </Button>
            </>
          ) : (
            <Button
              className="text-late min-h-10 px-3 text-sm"
              onClick={() => setConfirm(true)}
            >
              Delete…
            </Button>
          )}
        </div>
      )}
      <ErrorNote error={update.error ?? remove.error} />
    </li>
  );
}
