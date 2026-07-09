import { type BranchInfo, type WorktreeInfo } from "../lib/ipc";
import { relTime } from "../lib/relTime";

// The Worktrees lens: one row per worktree. Clicking a row focuses that worktree
// (its working changes + write actions + the current-branch marker follow it).
// ahead/behind and last-commit time are joined from the shared branch list.
export function WorktreePane({
  worktrees,
  branches,
  focusedPath,
  onFocus,
  filter,
}: {
  worktrees: WorktreeInfo[];
  branches: BranchInfo[];
  focusedPath: string;
  onFocus: (path: string) => void;
  filter: string;
}) {
  const f = filter.trim().toLowerCase();
  const branchByName = new Map(branches.filter((b) => !b.isRemote).map((b) => [b.name, b]));
  const shown = worktrees.filter(
    (w) => !f || (w.branch ?? "").toLowerCase().includes(f) || w.path.toLowerCase().includes(f),
  );

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
            {jb && (
              <span className="shrink-0 text-[10px] whitespace-nowrap text-faint tabular-nums">
                {relTime(jb.lastCommitTime)}
              </span>
            )}
          </div>
        );
      })}
    </div>
  );
}
