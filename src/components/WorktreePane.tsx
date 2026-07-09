import { useState, type MouseEvent } from "react";
import { openWorktree, type BranchInfo, type OpenTarget, type WorktreeInfo } from "../lib/ipc";
import { ContextMenu, type MenuItem } from "./ContextMenu";

// The Worktrees lens: one row per worktree. Clicking the name focuses that
// worktree (its working changes + write actions + the current-branch marker
// follow it); the ↗ opens it with the default target; right-click offers every
// "Open with…" target plus Copy path. ahead/behind are joined from the branch list.
export function WorktreePane({
  repoId,
  worktrees,
  branches,
  focusedPath,
  onFocus,
  filter,
  openTargets,
  defaultOpenTarget,
  onToast,
}: {
  repoId: string;
  worktrees: WorktreeInfo[];
  branches: BranchInfo[];
  focusedPath: string;
  onFocus: (path: string) => void;
  filter: string;
  openTargets: OpenTarget[];
  defaultOpenTarget: string | null;
  onToast: (msg: string) => void;
}) {
  const [menu, setMenu] = useState<{ x: number; y: number; wt: WorktreeInfo } | null>(null);

  const f = filter.trim().toLowerCase();
  const branchByName = new Map(branches.filter((b) => !b.isRemote).map((b) => [b.name, b]));
  const shown = worktrees.filter(
    (w) => !f || (w.branch ?? "").toLowerCase().includes(f) || w.path.toLowerCase().includes(f),
  );

  const open = (w: WorktreeInfo, targetId: string) =>
    openWorktree(repoId, w.path, targetId).catch((e: unknown) => onToast(String(e)));

  const openDefault = (w: WorktreeInfo) => {
    const id = defaultOpenTarget ?? openTargets[0]?.id;
    if (!id) return onToast("No open target configured — add one in Settings");
    void open(w, id);
  };

  const menuItems = (w: WorktreeInfo): MenuItem[] => [
    ...openTargets.map((t) => ({ label: `Open in ${t.name}`, onClick: () => void open(w, t.id) })),
    {
      label: "Copy path",
      onClick: () => {
        void navigator.clipboard.writeText(w.path);
        onToast("Path copied");
      },
    },
  ];

  const openMenu = (e: MouseEvent, w: WorktreeInfo) => {
    e.preventDefault();
    setMenu({ x: e.clientX, y: e.clientY, wt: w });
  };

  if (shown.length === 0) {
    return (
      <div className="px-2 py-3 text-[11px] text-faint">
        {f ? "No matching worktrees" : "No worktrees"}
      </div>
    );
  }

  return (
    <div>
      {shown.map((w) => {
        const focused = w.path === focusedPath;
        const jb = w.branch ? branchByName.get(w.branch) : undefined;
        return (
          <div
            key={w.path}
            role="button"
            title={w.path}
            onClick={() => onFocus(w.path)}
            onContextMenu={(e) => openMenu(e, w)}
            className={`flex h-7 cursor-default items-center gap-1.5 px-2 text-[12px] ${
              focused ? "bg-sel text-sel-fg" : "hover:bg-hover"
            } ${w.prunable ? "opacity-50" : ""}`}
          >
            <span className={`size-1.5 shrink-0 rounded-full ${focused ? "bg-good" : ""}`} />
            <span className="min-w-0 flex-1 truncate">
              {w.branch ?? w.head.slice(0, 7)}
              {!w.branch && <span className="ml-1 text-[10px] text-faint">detached</span>}
            </span>
            {w.isMain && <span className="shrink-0 text-[10px] text-faint">main</span>}
            {w.locked && <span className="shrink-0 text-[10px] text-faint">locked</span>}
            {w.dirty && (
              <span className="size-1.5 shrink-0 rounded-full bg-warn" title="Uncommitted changes" />
            )}
            {jb && (jb.ahead > 0 || jb.behind > 0) && (
              <span className="shrink-0 text-[10px] text-faint tabular-nums">
                {jb.ahead > 0 ? `↑${jb.ahead}` : ""}
                {jb.ahead > 0 && jb.behind > 0 ? " " : ""}
                {jb.behind > 0 ? `↓${jb.behind}` : ""}
              </span>
            )}
            <button
              className="shrink-0 rounded px-1 text-[12px] leading-none text-faint hover:bg-hover hover:text-fg"
              title="Open (default app)"
              onClick={(e) => {
                e.stopPropagation();
                openDefault(w);
              }}
            >
              ↗
            </button>
          </div>
        );
      })}
      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          items={menuItems(menu.wt)}
          onClose={() => setMenu(null)}
        />
      )}
    </div>
  );
}
