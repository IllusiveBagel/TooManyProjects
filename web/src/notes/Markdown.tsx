import { useMemo } from "react";
import ReactMarkdown, { defaultUrlTransform, type Components } from "react-markdown";
import { Link } from "react-router";
import remarkGfm from "remark-gfm";
import { NoteEmbed } from "../data/NoteEmbed";
import { noteFileUrl } from "../files/format";
import { EMBED_LANG, isImageName, readWikiHref, vaultFile, VAULT_PREFIX, wikiToMarkdown, type NoteLink } from "./format";

/**
 * A note's markdown, rendered. Wiki links go to the note or project they
 * name; a link to a note that doesn't exist yet is shown dashed and creates
 * it when clicked.
 */
export function NoteMarkdown({
  body,
  links,
  onCreate,
  projectIds = [],
  noteId,
}: {
  body: string;
  /** Lets `![[photo.jpg]]` and relative images load from the note's vault. */
  noteId?: string;
  links: NoteLink[];
  onCreate: (title: string) => void;
  /** The note's projects, so an embed finds their database first. */
  projectIds?: string[];
}) {
  const source = useMemo(() => wikiToMarkdown(body), [body]);
  const components: Components = useMemo(
    () => ({
      a({ href = "", children }) {
        const wiki = readWikiHref(href);
        if (!wiki) {
          return (
            <a href={href} target="_blank" rel="noreferrer noopener" className="text-accent underline decoration-1 underline-offset-4">
              {children}
            </a>
          );
        }
        const kind = wiki.project ? "project" : "note";
        const found = links.find((l) => l.kind === kind && l.title.toLowerCase() === wiki.title.toLowerCase());
        if (found?.id) {
          return (
            <Link
              to={wiki.project ? `/projects/${found.id}` : `/notes/${found.id}`}
              className={`border-b-[1.5px] no-underline ${wiki.project ? "text-slot-6 border-current/45" : "text-accent border-current/40"}`}
            >
              {children}
            </Link>
          );
        }
        if (wiki.project) return <span className="text-text-quiet">{children}</span>;
        return (
          <button
            type="button"
            title={`Create “${wiki.title}”`}
            onClick={() => onCreate(wiki.title)}
            className="text-text-quiet cursor-pointer border-b-[1.5px] border-dashed border-current"
          >
            {children}
          </button>
        );
      },
      pre({ node, children }) {
        const code = node?.children[0];
        const cls = code && "properties" in code ? code.properties?.className : undefined;
        if (Array.isArray(cls) && cls.includes(`language-${EMBED_LANG}`)) {
          const text = code && "children" in code ? code.children.map((c) => ("value" in c ? c.value : "")).join("") : "";
          return <NoteEmbed target={text.trim()} projectIds={projectIds} />;
        }
        return <pre>{children}</pre>;
      },
      img({ src = "", alt }) {
        const name = typeof src === "string" ? vaultFile(src) : null;
        if (!name || !noteId) return <img src={typeof src === "string" ? src : undefined} alt={alt ?? ""} />;
        if (!isImageName(name)) {
          return (
            <a href={noteFileUrl(noteId, name)} target="_blank" rel="noreferrer" className="border-rule label-mono inline-flex min-h-9 items-center gap-2 rounded-md px-3 text-[12px] no-underline">
              ▤ {name.split("/").pop()}
            </a>
          );
        }
        return (
          <a href={noteFileUrl(noteId, name)} target="_blank" rel="noreferrer" className="block">
            <img src={noteFileUrl(noteId, name, 1600)} alt={alt ?? name} loading="lazy" className="max-h-[70vh] rounded-md" />
          </a>
        );
      },
      input(props) {
        // GFM task list boxes: shown, not editable here.
        return <input {...props} disabled className="accent-accent mr-2 size-4 translate-y-0.5" />;
      },
    }),
    [links, onCreate, projectIds.join(","), noteId],
  );

  return (
    <div className="note-prose" data-testid="note-preview">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        components={components}
        urlTransform={(url) => (readWikiHref(url) || url.startsWith(VAULT_PREFIX) ? url : defaultUrlTransform(url))}
      >
        {source}
      </ReactMarkdown>
    </div>
  );
}
