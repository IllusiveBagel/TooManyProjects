import { useState } from "react";
import { useMe } from "../auth/queries";
import { dismissedVersion, dismissVersion, UPDATE_GUIDE_URL, useVersionInfo } from "./queries";

const linkClass = "label-mono text-text underline decoration-accent decoration-2 underline-offset-4 hover:text-accent";

/**
 * A strip under the top bar telling the owner a newer release is out. Closing
 * it hides that version for good; the next release brings it back.
 */
export function UpdateBanner() {
  const me = useMe();
  const info = useVersionInfo(me.role === "owner").data;
  const [dismissed, setDismissed] = useState(dismissedVersion);
  const latest = info?.latest;
  if (!info?.update_available || !latest || dismissed === latest.version) return null;

  return (
    <div
      role="status"
      className="border-rule bg-surface-raised flex items-center gap-3 border-x-0 border-t-0 px-4 py-2 text-sm sm:px-5"
    >
      <span aria-hidden="true" className="bg-accent size-2 shrink-0 rounded-[2px]" />
      <p className="min-w-0 flex-1">
        <span className="font-bold">Version {latest.version} is out.</span>{" "}
        <span className="text-text-quiet">
          You're on <span className="font-mono text-[13px]">{info.current}</span>.
        </span>{" "}
        <span className="mt-1 flex flex-wrap gap-x-4 gap-y-1 align-baseline sm:mt-0 sm:ml-2 sm:inline-flex">
          <a href={latest.url} target="_blank" rel="noreferrer" className={linkClass}>
            What's new
          </a>
          <a href={UPDATE_GUIDE_URL} target="_blank" rel="noreferrer" className={linkClass}>
            How to update
          </a>
        </span>
      </p>
      <button
        type="button"
        aria-label={`Hide the notice for version ${latest.version}`}
        title="Hide until the next release"
        onClick={() => {
          dismissVersion(latest.version);
          setDismissed(latest.version);
        }}
        className="text-text-quiet hover:text-text hover:bg-surface -my-1 -mr-2 grid size-10 shrink-0 cursor-pointer place-items-center rounded-md"
      >
        <svg viewBox="0 0 24 24" className="size-4" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true">
          <path d="M6 6l12 12M18 6L6 18" />
        </svg>
      </button>
    </div>
  );
}
