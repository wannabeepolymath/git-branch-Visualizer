import { useCallback, useEffect, useRef, useState, type MouseEvent as ReactMouseEvent } from "react";
import {
  addRepo,
  getBranches,
  getWorktrees,
  getSettings,
  normalizeTheme,
  onRepoChanged,
  pickRepoFolder,
  recenterWindow,
  setActiveRepo,
  type BranchInfo,
  type Settings,
  type WorktreeInfo,
} from "./lib/ipc";
import { BranchPane } from "./components/BranchPane";
import { CommitGraph } from "./components/CommitGraph";
import { Header } from "./components/Header";
import { SettingsView } from "./components/SettingsView";
import { Toast, useToast } from "./components/Toast";

const DEFAULT_SIDEBAR_WIDTH = 168;

export default function App() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [branches, setBranches] = useState<BranchInfo[]>([]);
  const [worktrees, setWorktrees] = useState<WorktreeInfo[]>([]);
  // Path of the worktree the app is acting on. "" means "not yet resolved" — the
  // worktrees fetch defaults it to the main worktree and self-heals if it vanishes.
  const [focusedWorktree, setFocusedWorktree] = useState<string>("");
  const [selectedRefs, setSelectedRefs] = useState<string[]>([]);
  const [view, setView] = useState<"main" | "settings">("main");
  const [refreshKey, setRefreshKey] = useState(0);
  const [showSidebar, setShowSidebar] = useState(() => localStorage.getItem("bv.sidebar") !== "0");
  const [sidebarWidth, setSidebarWidth] = useState(() => {
    const n = Number(localStorage.getItem("bv.sidebarWidth"));
    return Number.isFinite(n) && n >= 120 ? n : DEFAULT_SIDEBAR_WIDTH;
  });
  const startSidebarResize = (e: ReactMouseEvent) => {
    e.preventDefault();
    const startX = e.clientX;
    const startW = sidebarWidth;
    const move = (ev: MouseEvent) => {
      const max = Math.round(window.innerWidth * 0.7);
      setSidebarWidth(Math.min(Math.max(startW + ev.clientX - startX, 120), max));
    };
    const up = () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
      setSidebarWidth((w) => {
        localStorage.setItem("bv.sidebarWidth", String(w));
        return w;
      });
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  };
  const toggleSidebar = useCallback(
    () =>
      setShowSidebar((v) => {
        localStorage.setItem("bv.sidebar", v ? "0" : "1");
        return !v;
      }),
    [],
  );
  const { toast, show } = useToast();
  // Reset button: restore window size/position AND the branch panel width to defaults.
  const resetWindow = useCallback(() => {
    setSidebarWidth(DEFAULT_SIDEBAR_WIDTH);
    localStorage.setItem("bv.sidebarWidth", String(DEFAULT_SIDEBAR_WIDTH));
    recenterWindow().catch((e: unknown) => show(String(e)));
  }, [show]);

  const activeRepo = settings?.repos.find((r) => r.id === settings.activeRepoId) ?? null;
  const repoId = activeRepo?.id ?? null;
  const repoIdRef = useRef<string | null>(null);
  repoIdRef.current = repoId;

  const refresh = useCallback(() => setRefreshKey((k) => k + 1), []);

  // The worktree the app acts on. Pass undefined when it's the main worktree so
  // backend commands use the repo path directly and skip worktree validation.
  const focusedWt = worktrees.find((w) => w.path === focusedWorktree);
  const worktreeArg = focusedWt && !focusedWt.isMain ? focusedWt.path : undefined;
  const focusedWorktreeLabel = worktreeArg
    ? (focusedWt?.branch ?? focusedWt?.head.slice(0, 7))
    : undefined;
  const clearFocus = () => {
    const main = worktrees.find((w) => w.isMain);
    if (main) setFocusedWorktree(main.path);
  };

  // null clears to "all branches". additive (⌘/Ctrl-click) toggles a ref in the set; plain click focuses one.
  const selectRef = useCallback((ref: string | null, additive: boolean) => {
    if (ref === null) return setSelectedRefs([]);
    setSelectedRefs((cur) => {
      if (!additive) return [ref];
      return cur.includes(ref) ? cur.filter((r) => r !== ref) : [...cur, ref];
    });
  }, []);

  useEffect(() => {
    getSettings()
      .then(setSettings)
      .catch((e: unknown) => show(String(e)));
  }, [show]);

  // Theme: each visual identity is a full palette selected by `data-theme` on <html>.
  const theme = normalizeTheme(settings?.theme ?? "graphite");
  useEffect(() => {
    document.documentElement.setAttribute("data-theme", theme);
  }, [theme]);

  useEffect(() => {
    setSelectedRefs([]);
    setFocusedWorktree(""); // re-defaults to the new repo's main worktree on next fetch
  }, [repoId]);

  useEffect(() => {
    if (!repoId) {
      setBranches([]);
      return;
    }
    let live = true;
    getBranches(repoId)
      .then((b) => {
        if (live) setBranches(b);
      })
      .catch((e: unknown) => show(String(e)));
    return () => {
      live = false;
    };
  }, [repoId, refreshKey, settings?.showRemoteBranches, show]);

  useEffect(() => {
    if (!repoId) {
      setWorktrees([]);
      return;
    }
    let live = true;
    getWorktrees(repoId)
      .then((ws) => {
        if (!live) return;
        setWorktrees(ws);
        // Default focus to the main worktree; self-heal if the focused one is gone.
        setFocusedWorktree((cur) =>
          ws.some((w) => w.path === cur)
            ? cur
            : (ws.find((w) => w.isMain)?.path ?? ws[0]?.path ?? ""),
        );
      })
      .catch((e: unknown) => show(String(e)));
    return () => {
      live = false;
    };
  }, [repoId, refreshKey, show]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let dead = false;
    void onRepoChanged((changedId) => {
      if (changedId === repoIdRef.current) refresh();
    }).then((f) => {
      if (dead) f();
      else unlisten = f;
    });
    return () => {
      dead = true;
      unlisten?.();
    };
  }, [refresh]);

  const addRepository = useCallback(async () => {
    try {
      const path = await pickRepoFolder();
      if (!path) return;
      const repo = await addRepo(path);
      await setActiveRepo(repo.id);
      setSettings(await getSettings());
      setView("main");
    } catch (e) {
      show(String(e));
    }
  }, [show]);

  const switchRepo = useCallback(
    async (id: string) => {
      try {
        await setActiveRepo(id);
        setSettings(await getSettings());
      } catch (e) {
        show(String(e));
      }
    },
    [show],
  );

  if (!settings) {
    return (
      <main className="flex h-screen w-screen items-center justify-center bg-surface select-none">
        <span className="text-[12px] text-faint">Loading…</span>
      </main>
    );
  }

  return (
    <main className="flex h-screen w-screen flex-col overflow-hidden bg-surface text-[13px] text-fg select-none">
      <Header
        settings={settings}
        activeRepo={activeRepo}
        inSettings={view === "settings"}
        sidebarVisible={showSidebar}
        onToggleSidebar={toggleSidebar}
        onSwitchRepo={(id) => void switchRepo(id)}
        onAddRepo={() => void addRepository()}
        onToggleSettings={() => setView((v) => (v === "settings" ? "main" : "settings"))}
        onChanged={refresh}
        onToast={show}
        onResetWindow={resetWindow}
        focusedWorktreeLabel={focusedWorktreeLabel}
        onClearFocus={clearFocus}
      />
      {view === "settings" ? (
        <SettingsView
          settings={settings}
          onSettingsChange={setSettings}
          onAddRepo={() => void addRepository()}
          onToast={show}
        />
      ) : !activeRepo ? (
        <div className="flex flex-1 flex-col items-center justify-center gap-3">
          <div className="text-center">
            <div className="text-[13px] font-medium text-muted">No repository yet</div>
            <div className="mt-0.5 text-[11px] text-faint">
              Add a local git repository to get started
            </div>
          </div>
          <button
            className="rounded bg-accent px-3 py-1.5 text-[12px] font-medium text-accent-fg hover:opacity-90"
            onClick={() => void addRepository()}
          >
            Add repository
          </button>
        </div>
      ) : (
        <div className="flex min-h-0 flex-1">
          {showSidebar && (
            <>
              <div style={{ width: sidebarWidth }} className="flex min-w-0 shrink-0">
                <BranchPane
                  key={activeRepo.id}
                  repoId={activeRepo.id}
                  branches={branches}
                  worktrees={worktrees}
                  focusedWorktreePath={focusedWorktree}
                  onFocusWorktree={setFocusedWorktree}
                  selectedRefs={selectedRefs}
                  onSelect={selectRef}
                  showRemoteDefault={settings.showRemoteBranches}
                  onToast={show}
                  onChanged={refresh}
                />
              </div>
              <div
                role="separator"
                aria-orientation="vertical"
                aria-label="Resize branch panel"
                onMouseDown={startSidebarResize}
                className="-ml-[2px] w-[3px] shrink-0 cursor-col-resize hover:bg-accent/50 active:bg-accent/70"
              />
            </>
          )}
          <CommitGraph
            repoId={activeRepo.id}
            worktreePath={worktreeArg}
            refs={selectedRefs}
            pageSize={settings.commitsPerPage > 0 ? settings.commitsPerPage : 200}
            refreshKey={refreshKey}
            confirmActions={settings.confirmActions}
            theme={theme}
            onToast={show}
            onChanged={refresh}
          />
        </div>
      )}
      <Toast message={toast} />
    </main>
  );
}
