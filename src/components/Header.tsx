import { useRef, useState, type ReactNode } from "react";
import { fetchRepo, pullRepo, type RepoInfo, type Settings } from "../lib/ipc";
import { useDismiss } from "./ContextMenu";

function ChevronDownIcon() {
  return (
    <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="m6 9 6 6 6-6" />
    </svg>
  );
}

function RefreshIcon({ spinning }: { spinning: boolean }) {
  return (
    <svg
      width="13"
      height="13"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={spinning ? "animate-spin" : undefined}
    >
      <path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8" />
      <path d="M21 3v5h-5" />
      <path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16" />
      <path d="M8 16H3v5" />
    </svg>
  );
}

function PullIcon({ spinning }: { spinning: boolean }) {
  return (
    <svg
      width="13"
      height="13"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={spinning ? "animate-pulse" : undefined}
    >
      <path d="M12 5v13" />
      <path d="m6 12 6 6 6-6" />
    </svg>
  );
}

function PanelLeftIcon({ open }: { open: boolean }) {
  return (
    <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <rect width="18" height="18" x="3" y="3" rx="2" />
      <path d="M9 3v18" />
      {open && <rect width="3" height="12" x="4.5" y="6" rx="1" fill="currentColor" stroke="none" />}
    </svg>
  );
}

function BackIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <path d="m15 18-6-6 6-6" />
    </svg>
  );
}

function RecenterIcon() {
  return (
    <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <circle cx="12" cy="12" r="7" />
      <circle cx="12" cy="12" r="1.5" fill="currentColor" stroke="none" />
      <path d="M12 2v3M12 19v3M2 12h3M19 12h3" />
    </svg>
  );
}

function GearIcon() {
  return (
    <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" />
      <circle cx="12" cy="12" r="3" />
    </svg>
  );
}

const iconBtn =
  "flex size-6 items-center justify-center rounded text-muted hover:bg-hover hover:text-fg disabled:opacity-40 disabled:hover:bg-transparent";

function IconButton({
  label,
  active = false,
  disabled = false,
  tipLeft = false,
  onClick,
  children,
}: {
  label: string;
  active?: boolean;
  disabled?: boolean;
  tipLeft?: boolean; // align tooltip to the left edge (for buttons near the window's left)
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      className={`group relative ${iconBtn} ${active ? "bg-hover text-fg" : ""}`}
      aria-label={label}
      disabled={disabled}
      onClick={onClick}
    >
      {children}
      <span
        className={`pointer-events-none absolute top-full ${tipLeft ? "left-0" : "right-0"} z-50 mt-1 hidden rounded bg-fg px-1.5 py-0.5 text-[10px] whitespace-nowrap text-surface group-hover:block`}
      >
        {label}
      </span>
    </button>
  );
}

export function Header({
  settings,
  activeRepo,
  inSettings,
  sidebarVisible,
  onToggleSidebar,
  onSwitchRepo,
  onAddRepo,
  onToggleSettings,
  onChanged,
  onToast,
  onResetWindow,
  focusedWorktreeLabel,
  onClearFocus,
}: {
  settings: Settings;
  activeRepo: RepoInfo | null;
  inSettings: boolean;
  sidebarVisible: boolean;
  onToggleSidebar: () => void;
  onSwitchRepo: (repoId: string) => void;
  onAddRepo: () => void;
  onToggleSettings: () => void;
  onChanged: () => void;
  onToast: (msg: string) => void;
  onResetWindow: () => void;
  /** Branch/short-SHA of the focused worktree, when it isn't the main one. */
  focusedWorktreeLabel?: string;
  onClearFocus: () => void;
}) {
  const [open, setOpen] = useState(false);
  const [fetching, setFetching] = useState(false);
  const [pulling, setPulling] = useState(false);
  const switcherRef = useRef<HTMLDivElement>(null);
  useDismiss(switcherRef, () => setOpen(false));

  const doFetch = () => {
    if (!activeRepo || fetching) return;
    setFetching(true);
    fetchRepo(activeRepo.id)
      .then(() => {
        onToast("Fetched");
        onChanged();
      })
      .catch((e: unknown) => onToast(String(e)))
      .finally(() => setFetching(false));
  };

  const doPull = () => {
    if (!activeRepo || pulling) return;
    setPulling(true);
    pullRepo(activeRepo.id)
      .then(() => {
        onToast("Pulled");
        onChanged();
      })
      .catch((e: unknown) => onToast(String(e)))
      .finally(() => setPulling(false));
  };

  return (
    <header
      data-tauri-drag-region=""
      className="relative flex h-10 shrink-0 items-center gap-1 border-b border-edge px-2"
    >
      <IconButton label="Reset size & position" tipLeft onClick={onResetWindow}>
        <RecenterIcon />
      </IconButton>

      {inSettings ? (
        <IconButton label="Back" tipLeft onClick={onToggleSettings}>
          <BackIcon />
        </IconButton>
      ) : (
        <IconButton
          label={sidebarVisible ? "Hide branch panel" : "Show branch panel"}
          tipLeft
          disabled={!activeRepo}
          onClick={onToggleSidebar}
        >
          <PanelLeftIcon open={sidebarVisible} />
        </IconButton>
      )}

      {!inSettings && (
        <div ref={switcherRef} className="relative min-w-0">
        <button
          className="flex max-w-[240px] items-center gap-1.5 rounded px-1.5 py-1 text-[13px] font-medium hover:bg-hover"
          onClick={() => setOpen((o) => !o)}
        >
          <span className="truncate">{activeRepo ? activeRepo.name : "No repository"}</span>
          <span className="shrink-0 text-faint">
            <ChevronDownIcon />
          </span>
        </button>
        {open && (
          <div className="absolute top-full left-0 z-50 mt-1 w-64 rounded-md border border-edge bg-panel py-1 shadow-xl shadow-black/20">
            {settings.repos.length === 0 && (
              <div className="px-3 py-1.5 text-[11px] text-faint">No repositories yet</div>
            )}
            {settings.repos.map((r) => (
              <button
                key={r.id}
                className="block w-full px-3 py-1.5 text-left hover:bg-hover"
                onClick={() => {
                  setOpen(false);
                  if (r.id !== activeRepo?.id) onSwitchRepo(r.id);
                }}
              >
                <div
                  className={`truncate text-[12px] ${
                    r.id === activeRepo?.id
                      ? "font-semibold text-accent"
                      : "text-fg"
                  }`}
                >
                  {r.name}
                </div>
                <div className="truncate text-[10px] text-faint">{r.path}</div>
              </button>
            ))}
            <div className="my-1 border-t border-edge" />
            <button
              className="block w-full px-3 py-1.5 text-left text-[12px] text-accent hover:bg-hover"
              onClick={() => {
                setOpen(false);
                onAddRepo();
              }}
            >
              Add repository…
            </button>
          </div>
        )}
        </div>
      )}

      {!inSettings && focusedWorktreeLabel && (
        <button
          className="flex min-w-0 shrink items-center gap-1 rounded bg-accent-soft px-1.5 py-0.5 text-[11px] font-medium text-accent hover:opacity-80"
          onClick={onClearFocus}
          title="Focused worktree — click to return to the default worktree"
        >
          <span className="shrink-0">⧉</span>
          <span className="max-w-[120px] truncate">{focusedWorktreeLabel}</span>
          <span className="shrink-0 opacity-70">✕</span>
        </button>
      )}

      <div data-tauri-drag-region="" className="h-full flex-1" />

      {!inSettings && (
        <>
          <IconButton label="Fetch" disabled={!activeRepo || fetching} onClick={doFetch}>
            <RefreshIcon spinning={fetching} />
          </IconButton>
          <IconButton label="Pull" disabled={!activeRepo || pulling} onClick={doPull}>
            <PullIcon spinning={pulling} />
          </IconButton>
          <IconButton label="Settings" active={inSettings} onClick={onToggleSettings}>
            <GearIcon />
          </IconButton>
        </>
      )}
    </header>
  );
}
