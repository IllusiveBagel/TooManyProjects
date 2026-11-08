import { Button, ErrorNote, Panel } from "../components/ui";
import { ago } from "../notes/format";
import { UPDATE_GUIDE_URL, useCheckNow, useVersionInfo, type VersionInfo } from "./queries";

const linkClass = "label-mono text-text underline decoration-accent decoration-2 underline-offset-4 hover:text-accent";

/** Settings → Updates, for the owner: the running version and what the last check found. */
export function UpdatesPanel() {
  const info = useVersionInfo(true);
  const check = useCheckNow();
  const data = check.data ?? info.data;

  return (
    <Panel title="Updates">
      {info.isError && <ErrorNote error={info.error} />}
      {data && (
        <div className="flex flex-col gap-4">
          <div className="flex flex-wrap items-start gap-4">
            <div className="flex min-w-0 flex-1 flex-col gap-1">
              <p className="label-mono text-text-quiet">Running</p>
              <p className="font-mono text-[15px] font-bold">
                {data.current}
                {data.development_build && <span className="text-text-quiet font-normal"> · development build</span>}
              </p>
            </div>
            {data.check_enabled && (
              <Button type="button" onClick={() => check.mutate()} disabled={check.isPending}>
                {check.isPending ? "Checking…" : "Check now"}
              </Button>
            )}
          </div>
          <Status info={data} />
          {check.isError && <ErrorNote error={check.error} />}
          <p className="text-text-quiet border-rule border-x-0 border-b-0 pt-3 text-xs">
            {data.check_enabled ? (
              <>
                The server asks GitHub for the latest release a few times a day and sends nothing about this install.
                For an offline server, set <code className="font-mono">TMP_UPDATE_CHECK=off</code>.
              </>
            ) : (
              <>
                The update check is off because <code className="font-mono">TMP_UPDATE_CHECK</code> is set to off.
              </>
            )}
          </p>
        </div>
      )}
    </Panel>
  );
}

function Status({ info }: { info: VersionInfo }) {
  if (!info.check_enabled) return null;
  const checked = info.checked_at ? `Checked ${ago(info.checked_at)}.` : "Not checked yet.";
  if (info.error) return <p className="text-late text-sm">{info.error} {checked}</p>;
  const latest = info.latest;
  if (!latest) return <p className="text-text-quiet text-sm">No release has been published yet. {checked}</p>;
  if (info.update_available)
    return (
      <div className="border-rule bg-surface-raised flex flex-col gap-2 rounded-md p-3 text-sm">
        <p>
          <span className="font-bold">Version {latest.version} is out.</span>{" "}
          <span className="text-text-quiet">{checked}</span>
        </p>
        <p className="flex flex-wrap gap-x-4 gap-y-1">
          <a href={latest.url} target="_blank" rel="noreferrer" className={linkClass}>
            What's new
          </a>
          <a href={UPDATE_GUIDE_URL} target="_blank" rel="noreferrer" className={linkClass}>
            How to update
          </a>
        </p>
      </div>
    );
  if (info.development_build)
    return (
      <p className="text-text-quiet text-sm">
        The latest release is {latest.version}. A development build has no release version to compare, so no update
        notice is shown. {checked}
      </p>
    );
  return <p className="text-sm">Up to date. <span className="text-text-quiet">{checked}</span></p>;
}
