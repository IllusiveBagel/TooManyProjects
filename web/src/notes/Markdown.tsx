import { useMemo } from "react";
import ReactMarkdown, { defaultUrlTransform, type Components } from "react-markdown";
import { Link } from "react-router";
import remarkGfm from "remark-gfm";
import { readWikiHref, wikiToMarkdown, type NoteLink } from "./format";

/**
 * A note's markdown, rendered. Wiki links go to the note or project they
 * name; a link to a note that doesn't exist yet is shown dashed and creates
 * it when clicked.
 */
export function NoteMarkdown({
  body,
  links,
  onCreate,
}: {
  body: string;
  links: NoteLink[];
  onCreate: (title: string) => void;
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
      input(props) {
        // GFM task list boxes: shown, not editable here.
        return <input {...props} disabled className="accent-accent mr-2 size-4 translate-y-0.5" />;
      },
    }),
    [links, onCreate],
  );

  return (
    <div className="note-prose" data-testid="note-preview">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        components={components}
        urlTransform={(url) => (readWikiHref(url) ? url : defaultUrlTransform(url))}
      >
        {source}
      </ReactMarkdown>
    </div>
  );
}
