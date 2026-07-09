import { useState, type MouseEvent, type ReactNode } from "react";
import {
  checkout,
  createBranch,
  deleteBranch,
  renameBranch,
  type BranchInfo,
} from "../lib/ipc";
import { relTime } from "../lib/relTime";
import { ContextMenu, PromptPopover, type MenuItem } from "./ContextMenu";

type PromptKind = "new" | "rename" | "delete";

function Group({
  label,
  count,
  open,
  onToggle,
  children,
}: {
  label: string;
  count: number;
  open: boolean;
  onToggle: () => void;
  children: ReactNode;
}) {
  return (
    <div>
      <button
        className="flex w-full items-center gap-1 px-2 py-1 text-[10px] font-semibold tracking-wider text-neutral-400 uppercase hover:text-neutral-600 dark:hover:text-neutral-300"
        onClick={onToggle}
      >
        <svg
          width="8"
          height="8"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="3"
          strokeLinecap="round"
          strokeLinejoin="round"
          className={open ? "rotate-90" : ""}
        >
          <path d="m9 18 6-6-6-6" />
        </svg>
        {label}
        <span className="font-normal tabular-nums">{count}</span>
      </button>
      {open && children}
    </div>
  );
}

export function BranchPane({
  repoId,
  branches,
  selectedRefs,
  onSelect,
  showRemoteDefault,
  onToast,
  onChanged,
}: {
  repoId: string;
  branches: BranchInfo[];
  selectedRefs: string[];
  onSelect: (ref: string | null, additive: boolean) => void;
  showRemoteDefault: boolean;
  onToast: (msg: string) => void;
  onChanged: () => void;
}) {
  const [filter, setFilter] = useState("");
  const [openLocal, setOpenLocal] = useState(true);
  const [openRemote, setOpenRemote] = useState(showRemoteDefault);
  const [menu, setMenu] = useState<{ x: number; y: number; branch: BranchInfo } | null>(null);
  const [prompt, setPrompt] = useState<{
    x: number;
    y: number;
    kind: PromptKind;
    branch: BranchInfo;
  } | null>(null);

  const f = filter.trim().toLowerCase();
  const match = (b: BranchInfo) => b.name.toLowerCase().includes(f);
  const current = branches.filter((b) => b.isCurrent && match(b));
  const local = branches.filter((b) => !b.isCurrent && !b.isRemote && match(b));
  const remote = branches.filter((b) => b.isRemote && match(b));

  const run = (p: Promise<void>, ok: string) =>
    p
      .then(() => {
        onToast(ok);
        onChanged();
      })
      .catch((e: unknown) => onToast(String(e)));

  const openMenu = (e: MouseEvent, b: BranchInfo) => {
    e.preventDefault();
    setMenu({ x: e.clientX, y: e.clientY, branch: b });
  };

  const menuItems = (m: { x: number; y: number; branch: BranchInfo }): MenuItem[] => {
    const b = m.branch;
    const items: MenuItem[] = [
      { label: "Checkout", onClick: () => run(checkout(repoId, b.name), `Checked out ${b.name}`) },
      {
        label: "New branch from here…",
        onClick: () => setPrompt({ x: m.x, y: m.y, kind: "new", branch: b }),
      },
    ];
    if (!b.isRemote) {
      items.push(
        {
          label: "Rename…",
          onClick: () => setPrompt({ x: m.x, y: m.y, kind: "rename", branch: b }),
        },
        {
          label: "Delete…",
          danger: true,
          onClick: () => setPrompt({ x: m.x, y: m.y, kind: "delete", branch: b }),
        },
      );
    }
    items.push({
      label: "Copy name",
      onClick: () => {
        void navigator.clipboard.writeText(b.name);
        onToast("Name copied");
      },
    });
    return items;
  };

  const row = (b: BranchInfo) => {
    const selected = selectedRefs.includes(b.name);
    return (
      <div
        key={b.name}
        role="button"
        className={`flex h-7 cursor-default items-center gap-1.5 px-2 text-[12px] ${
          selected
            ? "bg-blue-500/10 text-blue-700 dark:bg-blue-400/10 dark:text-blue-300"
            : "hover:bg-neutral-100 dark:hover:bg-neutral-800"
        }`}
        title={b.lastCommitSubject}
        onClick={(e) => onSelect(b.name, e.metaKey || e.ctrlKey)}
        onContextMenu={(e) => openMenu(e, b)}
      >
        {b.isCurrent && <span className="size-1.5 shrink-0 rounded-full bg-green-500" />}
        <span className="min-w-0 flex-1 truncate">{b.name}</span>
        {(b.ahead > 0 || b.behind > 0) && (
          <span className="shrink-0 text-[10px] text-neutral-400 tabular-nums">
            {b.ahead > 0 ? `↑${b.ahead}` : ""}
            {b.ahead > 0 && b.behind > 0 ? " " : ""}
            {b.behind > 0 ? `↓${b.behind}` : ""}
          </span>
        )}
        <span className="shrink-0 text-[10px] whitespace-nowrap text-neutral-400 tabular-nums">
          {relTime(b.lastCommitTime)}
        </span>
      </div>
    );
  };

  return (
    <div className="flex w-full min-w-0 flex-col border-r border-neutral-200 dark:border-neutral-800">
      <div className="p-1.5">
        <input
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          placeholder="Filter branches…"
          className="w-full rounded border border-neutral-200 bg-neutral-50 px-2 py-1 text-[12px] outline-none placeholder:text-neutral-400 focus:border-blue-500 dark:border-neutral-700 dark:bg-neutral-800/60"
        />
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto pb-2">
        <div
          role="button"
          className={`flex h-7 cursor-default items-center px-2 text-[12px] ${
            selectedRefs.length === 0
              ? "bg-blue-500/10 text-blue-700 dark:bg-blue-400/10 dark:text-blue-300"
              : "hover:bg-neutral-100 dark:hover:bg-neutral-800"
          }`}
          onClick={() => onSelect(null, false)}
        >
          All branches
        </div>

        {current.length > 0 && (
          <>
            <div className="px-2 py-1 text-[10px] font-semibold tracking-wider text-neutral-400 uppercase">
              Current
            </div>
            {current.map(row)}
          </>
        )}

        <Group label="Local" count={local.length} open={openLocal} onToggle={() => setOpenLocal((o) => !o)}>
          {local.length === 0 ? (
            <div className="px-2 py-1 text-[11px] text-neutral-400">
              {f ? "No matches" : "No local branches"}
            </div>
          ) : (
            local.map(row)
          )}
        </Group>

        <Group
          label="Remotes"
          count={remote.length}
          open={openRemote}
          onToggle={() => setOpenRemote((o) => !o)}
        >
          {remote.length === 0 ? (
            <div className="px-2 py-1 text-[11px] text-neutral-400">
              {f ? "No matches" : "No remote branches"}
            </div>
          ) : (
            remote.map(row)
          )}
        </Group>
      </div>

      {menu && <ContextMenu x={menu.x} y={menu.y} items={menuItems(menu)} onClose={() => setMenu(null)} />}

      {prompt &&
        (() => {
          const b = prompt.branch;
          const common = { x: prompt.x, y: prompt.y, onClose: () => setPrompt(null) };
          if (prompt.kind === "new") {
            return (
              <PromptPopover
                {...common}
                title={`New branch from ${b.name}`}
                withInput
                placeholder="branch name"
                confirmLabel="Create"
                onConfirm={async (v) => {
                  await createBranch(repoId, v, b.name);
                  onToast(`Created ${v}`);
                  onChanged();
                }}
              />
            );
          }
          if (prompt.kind === "rename") {
            return (
              <PromptPopover
                {...common}
                title="Rename branch"
                withInput
                initial={b.name}
                confirmLabel="Rename"
                onConfirm={async (v) => {
                  await renameBranch(repoId, b.name, v);
                  onToast(`Renamed to ${v}`);
                  onChanged();
                }}
              />
            );
          }
          return (
            <PromptPopover
              {...common}
              title={`Delete "${b.name}"?`}
              confirmLabel="Delete"
              danger
              onConfirm={async () => {
                await deleteBranch(repoId, b.name);
                onToast(`Deleted ${b.name}`);
                onChanged();
              }}
            />
          );
        })()}
    </div>
  );
}
