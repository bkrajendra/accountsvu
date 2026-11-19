import { createContext, useCallback, useContext, useEffect, useRef, useState, type ReactNode } from "react";
import { Download, ExternalLink, Loader2 } from "lucide-react";
import { api, ApiError, type UpdateInfo, type UpdateProgress } from "./api";
import { useToast } from "../components/Toast";
import { Dialog } from "../components/Dialog";
import { Button } from "../components/ui";

export type UpdatePhase = "idle" | "checking" | "current" | "available" | "downloading" | "installing" | "error";

interface Updater {
  phase: UpdatePhase;
  info: UpdateInfo | null;
  checkedAt: Date | null;
  /** Bytes downloaded and total (total may be unknown). */
  progress: { downloaded: number; total: number | null } | null;
  error: string | null;
  check: () => Promise<UpdateInfo | null>;
  install: () => Promise<void>;
  dialogOpen: boolean;
  openDialog: () => void;
  closeDialog: () => void;
}

const UpdaterContext = createContext<Updater | null>(null);

/**
 * Update state lives above the lock gate so locking the app doesn't lose a
 * download in progress or re-run the startup check.
 */
export function UpdaterProvider({ children }: { children: ReactNode }) {
  const [phase, setPhase] = useState<UpdatePhase>("idle");
  const [info, setInfo] = useState<UpdateInfo | null>(null);
  const [checkedAt, setCheckedAt] = useState<Date | null>(null);
  const [progress, setProgress] = useState<Updater["progress"]>(null);
  const [error, setError] = useState<string | null>(null);
  const [dialogOpen, setDialogOpen] = useState(false);

  const check = useCallback(async () => {
    setPhase("checking");
    setError(null);
    try {
      const found = await api.checkUpdate();
      setInfo(found);
      setCheckedAt(new Date());
      setPhase(found ? "available" : "current");
      return found;
    } catch (e) {
      setError(e instanceof ApiError ? e.message : "Couldn't check for updates.");
      setPhase("error");
      return null;
    }
  }, []);

  const install = useCallback(async () => {
    setPhase("downloading");
    setError(null);
    setProgress({ downloaded: 0, total: null });
    const onProgress = (p: UpdateProgress) => {
      if (p.event === "started") setProgress({ downloaded: 0, total: p.contentLength });
      else if (p.event === "progress") setProgress((x) => ({ downloaded: p.downloaded, total: x?.total ?? null }));
      else setPhase("installing");
    };
    try {
      await api.installUpdate(onProgress);
    } catch (e) {
      setError(e instanceof ApiError ? e.message : "The update couldn't be installed.");
      // The downloaded update was used up; a fresh check finds it again.
      setPhase("error");
      setProgress(null);
    }
  }, []);

  return (
    <UpdaterContext.Provider
      value={{
        phase,
        info,
        checkedAt,
        progress,
        error,
        check,
        install,
        dialogOpen,
        openDialog: () => setDialogOpen(true),
        closeDialog: () => setDialogOpen(false),
      }}
    >
      {children}
    </UpdaterContext.Provider>
  );
}

export function useUpdater(): Updater {
  const u = useContext(UpdaterContext);
  if (!u) throw new Error("useUpdater outside UpdaterProvider");
  return u;
}

let startupCheckDone = false;

/**
 * Lives inside the lock gate: checks once per run, a few seconds after the
 * app opens, offers the update in a toast, and hosts the update dialog.
 */
export function UpdateHost() {
  const u = useUpdater();
  const toast = useToast();
  const { check, openDialog } = u;
  const timer = useRef<number | undefined>(undefined);

  useEffect(() => {
    if (startupCheckDone) return;
    timer.current = window.setTimeout(() => {
      startupCheckDone = true;
      void check().then((found) => {
        if (found) toast(`AccountsVU ${found.version} is available`, "info", { label: "Update", onClick: openDialog });
      });
    }, 3000);
    return () => window.clearTimeout(timer.current);
  }, [check, openDialog, toast]);

  return <UpdateDialog />;
}

function mb(bytes: number) {
  return (bytes / 1_048_576).toFixed(1) + " MB";
}

function UpdateDialog() {
  const { dialogOpen, closeDialog, info, phase, progress, error, install, check } = useUpdater();
  const busy = phase === "downloading" || phase === "installing";
  const pct = progress?.total ? Math.min(100, Math.round((progress.downloaded / progress.total) * 100)) : null;

  if (!info && phase !== "error") return null;
  const close = () => {
    if (!busy) closeDialog();
  };

  return (
    <Dialog
      open={dialogOpen}
      onClose={close}
      width={480}
      title={info ? `Update to AccountsVU ${info.version}` : "Update"}
      description={info ? `You have version ${info.currentVersion}.` : undefined}
      footer={
        <>
          <Button onClick={close} disabled={busy}>
            Later
          </Button>
          {info && !info.canInstall ? (
            <Button variant="primary" onClick={() => void api.openReleasesPage()}>
              <ExternalLink size={15} /> Download page
            </Button>
          ) : phase === "error" && !info ? null : phase === "error" ? (
            <>
              <Button onClick={() => void api.openReleasesPage()}>
                <ExternalLink size={15} /> Download page
              </Button>
              <Button variant="primary" onClick={() => void check()}>
                Try again
              </Button>
            </>
          ) : (
            <Button variant="primary" disabled={busy} onClick={() => void install()}>
              {busy ? <Loader2 size={15} className="animate-spin" /> : <Download size={15} />}
              {phase === "installing" ? "Installing…" : busy ? "Downloading…" : "Update now"}
            </Button>
          )}
        </>
      }
    >
      <div className="space-y-4 text-[13.5px]">
        {info?.notes && (
          <div className="max-h-40 overflow-y-auto whitespace-pre-wrap rounded-xl bg-surface-2 px-4 py-3 text-muted">
            {info.notes}
          </div>
        )}
        {busy ? (
          <div>
            <div
              className="h-2 overflow-hidden rounded-full bg-active"
              role="progressbar"
              aria-label="Download progress"
              aria-valuemin={0}
              aria-valuemax={100}
              aria-valuenow={pct ?? undefined}
            >
              <div
                className="h-full rounded-full bg-accent-fill transition-[width] duration-200 ease-out"
                style={{ width: `${phase === "installing" ? 100 : (pct ?? 8)}%` }}
              />
            </div>
            <p className="mt-2 text-[12.5px] text-muted">
              {phase === "installing"
                ? "Installing. AccountsVU will restart in a moment."
                : progress?.total
                  ? `${mb(progress.downloaded)} of ${mb(progress.total)}`
                  : progress
                    ? mb(progress.downloaded)
                    : "Starting download…"}
            </p>
          </div>
        ) : error ? (
          <p className="text-danger">{error}</p>
        ) : info && !info.canInstall ? (
          <p className="text-muted">
            This copy was installed from a .deb or .rpm package, which can't update itself. Download the new package from the
            releases page.
          </p>
        ) : (
          <p className="text-muted">
            The update is downloaded, its signature checked, and AccountsVU restarts to finish. Your data stays where it is.
          </p>
        )}
      </div>
    </Dialog>
  );
}
