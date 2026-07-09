import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type MouseEvent,
  type ReactNode,
  type UIEvent,
} from "react";
import {
  checkout,
  createBranch,
  diffCommitFile,
  diffFile,
  discardFiles,
  getCommit,
  getLog,
  getStatus,
  stageFiles,
  unstageFiles,
  type CommitDetail,
  type CommitInfo,
  type FileChange,
  type ThemeName,
  type WorkingStatus,
} from "../lib/ipc";
import { laneColor, layoutGraph, type GraphRow } from "../lib/graph";
import { relTime } from "../lib/relTime";
import { ContextMenu, PromptPopover, type MenuItem } from "./ContextMenu";

const ROW_H = 28;
const LANE_W = 12;
const DETAIL_H = 236;
const OVERSCAN = 8;
const MAX_RAIL_LANES = 8; // rail column stops growing past this; extra lanes clip

function railX(lane: number): number {
  return 6 + lane * LANE_W;
}

function Rail({ row, railW, theme }: { row: GraphRow; railW: number; theme: ThemeName }) {
  const cx = railX(row.lane);
  const cy = ROW_H / 2;
  const paths: ReactNode[] = [];
  for (const j of row.passes) {
    paths.push(<path key={`p${j}`} d={`M ${railX(j)} 0 V ${ROW_H}`} stroke={laneColor(j, theme)} />);
  }
  for (const j of row.ins) {
    const d =
      j === row.lane
        ? `M ${cx} 0 V ${cy}`
        : `M ${railX(j)} 0 Q ${railX(j)} ${cy} ${cx} ${cy}`;
    paths.push(<path key={`i${j}`} d={d} stroke={laneColor(j, theme)} />);
  }
  for (const j of row.outs) {
    const d =
      j === row.lane
        ? `M ${cx} ${cy} V ${ROW_H}`
        : `M ${cx} ${cy} Q ${railX(j)} ${cy} ${railX(j)} ${ROW_H}`;
    paths.push(<path key={`o${j}`} d={d} stroke={laneColor(j, theme)} />);
  }
  const isMerge = row.outs.length > 1;
  const c = laneColor(row.lane, theme);
  // Terminal reads as a TUI: square nodes with a faint neon glow.
  const glow = theme === "terminal" ? { filter: `drop-shadow(0 0 1.8px ${c})` } : undefined;
  const node =
    theme === "terminal" ? (
      isMerge ? (
        <rect x={cx - 3} y={cy - 3} width={6} height={6} strokeWidth={1.6} stroke={c} className="fill-surface" style={glow} />
      ) : (
        <rect x={cx - 3} y={cy - 3} width={6} height={6} fill={c} style={glow} />
      )
    ) : isMerge ? (
      <circle cx={cx} cy={cy} r={3} strokeWidth={1.5} stroke={c} className="fill-surface" />
    ) : (
      <circle cx={cx} cy={cy} r={3} fill={c} />
    );
  return (
    <svg width={railW} height={ROW_H} className="shrink-0 overflow-hidden">
      <g fill="none" strokeWidth={1.5}>
        {paths}
      </g>
      {node}
    </svg>
  );
}

// `full` drops the inline width cap so the whole ref name shows — used in the
// overflow popover where there's no neighboring content to crowd.
function RefPill({ r, theme, full }: { r: string; theme: ThemeName; full?: boolean }) {
  const isTag = r.startsWith("tag:");
  const label = isTag ? r.slice(4) : r;
  // Terminal drops the pill chrome for bracketed tags — [branch] / <tag>.
  if (theme === "terminal") {
    return (
      <span
        title={label}
        className={`font-mono text-[9.5px] leading-[14px] font-semibold ${
          full ? "whitespace-nowrap" : "max-w-[110px] shrink-0 truncate"
        } ${isTag ? "text-tag-fg" : "text-pill-fg"}`}
      >
        {isTag ? `<${label}>` : `[${label}]`}
      </span>
    );
  }
  return (
    <span
      title={label}
      className={`rounded-sm px-1 text-[9px] leading-[14px] font-medium ${
        full ? "whitespace-nowrap" : "max-w-[76px] shrink-0 truncate"
      } ${isTag ? "bg-tag text-tag-fg" : "bg-pill text-pill-fg"}`}
    >
      {label}
    </span>
  );
}

function statusColor(status: string): string {
  switch (status.charAt(0)) {
    case "A":
      return "text-add";
    case "D":
      return "text-del";
    case "M":
      return "text-mod";
    case "R":
    case "C":
      return "text-ren";
    case "?":
      return "text-add"; // untracked (new file)
    default:
      return "text-muted";
  }
}

function FileRow({
  f,
  actions,
  actionsVisible,
  onToggleDiff,
}: {
  f: FileChange;
  actions?: ReactNode;
  actionsVisible?: boolean; // keep actions shown even without hover (confirm armed)
  onToggleDiff?: () => void;
}) {
  return (
    <div
      className={`group flex items-center gap-1.5 py-px text-[11px] ${
        onToggleDiff ? "cursor-pointer" : ""
      }`}
      onClick={onToggleDiff}
    >
      <span className={`w-3 shrink-0 text-center font-mono font-semibold ${statusColor(f.status)}`}>
        {f.status.charAt(0)}
      </span>
      <span className="min-w-0 flex-1 truncate text-muted" title={f.path}>
        {f.path}
      </span>
      {actions && (
        <span
          className={`flex shrink-0 items-center gap-1 ${
            actionsVisible ? "" : "opacity-0 group-hover:opacity-100"
          }`}
          onClick={(e) => e.stopPropagation()}
        >
          {actions}
        </span>
      )}
    </div>
  );
}

function ActBtn({
  label,
  title,
  onClick,
  danger,
}: {
  label: string;
  title: string;
  onClick: () => void;
  danger?: boolean;
}) {
  return (
    <button
      title={title}
      onClick={onClick}
      className={`rounded px-1 text-[10px] leading-[15px] ${
        danger ? "text-del hover:bg-del/10" : "text-muted hover:bg-hover"
      }`}
    >
      {label}
    </button>
  );
}

function diffLineClass(ln: string): string {
  if (ln.startsWith("@@")) return "text-accent";
  if (ln.startsWith("+++") || ln.startsWith("---")) return "text-faint";
  if (/^(diff |index |new file|deleted file|rename |similarity |Binary )/.test(ln))
    return "text-faint";
  if (ln.startsWith("+")) return "text-add";
  if (ln.startsWith("-")) return "text-del";
  return "text-muted";
}

const DIFF_CAP = 600; // lines rendered before truncation

function DiffView({ text }: { text: string | null }) {
  if (text === null)
    return <div className="py-1 pl-5 text-[11px] text-faint">Loading diff…</div>;
  if (text.trim() === "")
    return <div className="py-1 pl-5 text-[11px] text-faint">No textual changes</div>;
  const lines = text.split("\n");
  const shown = lines.slice(0, DIFF_CAP);
  return (
    <pre className="mt-0.5 mb-1 ml-5 overflow-x-auto rounded border border-edge bg-surface px-2 py-1 font-mono text-[10.5px] leading-[15px] select-text">
      {shown.map((ln, i) => (
        <div key={i} className={diffLineClass(ln)}>
          {ln || " "}
        </div>
      ))}
      {lines.length > DIFF_CAP && (
        <div className="text-faint">… diff truncated ({lines.length - DIFF_CAP} more lines)</div>
      )}
    </pre>
  );
}

function DetailPanel({
  top,
  repoId,
  detail,
  onToast,
}: {
  top: number;
  repoId: string;
  detail: CommitDetail | null;
  onToast: (msg: string) => void;
}) {
  const [openDiff, setOpenDiff] = useState<string | null>(null);
  const [diffText, setDiffText] = useState<string | null>(null);

  // Collapse any open diff when the panel switches to another commit.
  const hash = detail?.hash;
  useEffect(() => {
    setOpenDiff(null);
    setDiffText(null);
  }, [hash]);

  const toggleDiff = (path: string) => {
    if (!hash) return;
    if (openDiff === path) {
      setOpenDiff(null);
      setDiffText(null);
      return;
    }
    setOpenDiff(path);
    setDiffText(null); // DiffView renders its loading state on null
    diffCommitFile(repoId, hash, path)
      .then(setDiffText)
      .catch((e: unknown) => {
        onToast(String(e));
        setOpenDiff(null);
      });
  };

  return (
    <div
      className="absolute inset-x-0 z-10 overflow-y-auto border-y border-edge bg-panel2 px-3 py-2"
      style={{ top, height: DETAIL_H }}
      onContextMenu={(e) => e.stopPropagation()}
    >
      {!detail ? (
        <div className="text-[11px] text-faint">Loading…</div>
      ) : (
        <>
          <div className="flex items-center gap-2">
            <span className="font-mono text-[11px] text-muted select-text">
              {detail.hash.slice(0, 12)}
            </span>
            <button
              className="rounded border border-edge px-1.5 text-[10px] leading-[16px] text-muted hover:bg-hover"
              onClick={() => {
                void navigator.clipboard.writeText(detail.hash);
                onToast("Hash copied");
              }}
            >
              Copy hash
            </button>
          </div>
          <div className="mt-1.5 text-[12px] font-medium select-text">{detail.subject}</div>
          {detail.body.trim() !== "" && (
            <pre className="mt-1 font-sans text-[11.5px] whitespace-pre-wrap text-muted select-text">
              {detail.body.trim()}
            </pre>
          )}
          <div className="mt-1.5 text-[11px] text-muted select-text">
            {`${detail.authorName} <${detail.authorEmail}>`}
            <span className="mx-1 text-faint">·</span>
            {new Date(detail.timestamp * 1000).toLocaleString()}
          </div>
          <div className="mt-2 border-t border-edge pt-1.5">
            {detail.files.length === 0 ? (
              <div className="text-[11px] text-faint">No files changed</div>
            ) : (
              detail.files.map((f) => (
                <div key={f.path}>
                  <FileRow f={f} onToggleDiff={() => toggleDiff(f.path)} />
                  {openDiff === f.path && <DiffView text={diffText} />}
                </div>
              ))
            )}
          </div>
        </>
      )}
    </div>
  );
}

/**
 * Pinned, expandable working-tree panel: staged (index) and unstaged (worktree +
 * untracked) changes. Hidden when the tree is clean. Rows can be staged/unstaged,
 * discarded (with a confirm — destructive), and expanded to show their diff.
 * Refetches on repo change, external refresh, and each time it's opened — the
 * .git watcher misses raw file edits, so opening acts as a manual refresh.
 */
function WorkingChanges({
  repoId,
  worktreePath,
  refreshKey,
  confirmActions,
  onToast,
  onChanged,
}: {
  repoId: string;
  worktreePath?: string;
  refreshKey: number;
  confirmActions: boolean;
  onToast: (msg: string) => void;
  onChanged: () => void;
}) {
  const [status, setStatus] = useState<WorkingStatus | null>(null);
  const [open, setOpen] = useState(false);
  const [openDiff, setOpenDiff] = useState<string | null>(null); // "s:path" / "u:path"
  const [diffText, setDiffText] = useState<string | null>(null);
  // A codebase-affecting action is armed and awaiting tick/cross confirmation.
  // Only one at a time; `run` carries the intent, `label` names it, `danger` reddens the tick.
  const [pending, setPending] = useState<{
    id: string;
    label: string;
    run: () => void;
    danger?: boolean;
  } | null>(null);
  const openDiffRef = useRef<string | null>(null);
  openDiffRef.current = openDiff;

  const load = useCallback(() => {
    getStatus(repoId, worktreePath)
      .then(setStatus)
      .catch((e: unknown) => onToast(String(e)));
  }, [repoId, worktreePath, onToast]);

  useEffect(() => {
    load();
  }, [load, refreshKey]);
  // A reload can move/remove rows; collapse any open diff and disarm any pending
  // confirm so neither points at a now-stale row.
  useEffect(() => {
    setOpenDiff(null);
    setDiffText(null);
    setPending(null);
  }, [status]);

  const staged = status?.staged ?? [];
  const unstaged = status?.unstaged ?? [];
  if (staged.length + unstaged.length === 0) return null;

  const act = (p: Promise<void>, ok: string) =>
    p.then(() => {
      onToast(ok);
      onChanged();
    }).catch((e: unknown) => onToast(String(e)));

  // Confirmation on → arm a tick/cross; off → run the action immediately.
  const arm = (o: NonNullable<typeof pending>) => (confirmActions ? setPending(o) : o.run());

  const toggleDiff = (key: string, f: FileChange, isStaged: boolean) => {
    if (openDiff === key) {
      setOpenDiff(null);
      setDiffText(null);
      return;
    }
    setOpenDiff(key);
    setDiffText(null);
    diffFile(repoId, f.path, isStaged, f.status === "?", worktreePath)
      .then((t) => {
        if (openDiffRef.current === key) setDiffText(t);
      })
      .catch((e: unknown) => onToast(String(e)));
  };

  // Tick/cross that replaces an action's button(s) once armed.
  const confirmUI = (p: NonNullable<typeof pending>) => (
    <>
      <span className="text-[10px] text-faint">{p.label}?</span>
      <button
        title="Confirm"
        onClick={(e) => {
          e.stopPropagation();
          setPending(null);
          p.run();
        }}
        className={`rounded px-1 text-[11px] leading-[15px] ${
          p.danger ? "text-del hover:bg-del/10" : "text-good hover:bg-good/10"
        }`}
      >
        ✓
      </button>
      <button
        title="Cancel"
        onClick={(e) => {
          e.stopPropagation();
          setPending(null);
        }}
        className="rounded px-1 text-[11px] leading-[15px] text-muted hover:bg-hover"
      >
        ✕
      </button>
    </>
  );

  const renderFile = (f: FileChange, isStaged: boolean) => {
    const key = `${isStaged ? "s" : "u"}:${f.path}`;
    const armed = pending?.id === key;
    const actions = armed ? (
      confirmUI(pending)
    ) : isStaged ? (
      <ActBtn
        label="Unstage"
        title="Unstage"
        onClick={() =>
          arm({
            id: key,
            label: "Unstage",
            run: () => act(unstageFiles(repoId, [f.path], worktreePath), "Unstaged"),
          })
        }
      />
    ) : (
      <>
        <ActBtn
          label="Stage"
          title="Stage"
          onClick={() =>
            arm({
              id: key,
              label: "Stage",
              run: () => act(stageFiles(repoId, [f.path], worktreePath), "Staged"),
            })
          }
        />
        <ActBtn
          label="Discard"
          title="Discard changes"
          danger
          onClick={() =>
            arm({
              id: key,
              label: "Discard",
              danger: true,
              run: () => act(discardFiles(repoId, [f.path], f.status === "?", worktreePath), "Discarded"),
            })
          }
        />
      </>
    );
    return (
      <div key={key}>
        <FileRow
          f={f}
          actions={actions}
          actionsVisible={armed}
          onToggleDiff={() => toggleDiff(key, f, isStaged)}
        />
        {openDiff === key && <DiffView text={diffText} />}
      </div>
    );
  };

  const sectionHeader = (id: string, label: string, allLabel: string, onAll: () => void, mt: boolean) => (
    <div className={`mb-0.5 flex items-center gap-1.5 ${mt ? "mt-2" : ""}`}>
      <span className="text-[10px] font-semibold tracking-wide text-faint uppercase">
        {label}
      </span>
      {pending?.id === id ? (
        confirmUI(pending)
      ) : (
        <button
          className="text-[10px] text-faint hover:text-fg"
          onClick={() => arm({ id, label: allLabel, run: onAll })}
        >
          {allLabel}
        </button>
      )}
    </div>
  );

  return (
    <div className="shrink-0 border-b border-edge">
      <button
        className="flex w-full items-center gap-2 px-3 py-1.5 text-left hover:bg-hover"
        onClick={() => setOpen((v) => (v ? false : (load(), true)))}
      >
        <span className="w-2 shrink-0 text-[9px] text-faint">{open ? "▾" : "▸"}</span>
        <span className="h-2 w-2 shrink-0 rounded-full bg-warn" />
        <span className="text-[12px] font-medium">Uncommitted changes</span>
        <span className="text-[11px] text-faint">
          {[
            staged.length > 0 ? `${staged.length} staged` : null,
            unstaged.length > 0 ? `${unstaged.length} unstaged` : null,
          ]
            .filter(Boolean)
            .join(" · ")}
        </span>
      </button>
      {open && (
        <div className="max-h-[360px] overflow-y-auto border-t border-edge bg-panel2 px-3 py-2">
          {staged.length > 0 && (
            <>
              {sectionHeader(
                "all:staged",
                "Staged",
                "Unstage all",
                () => act(unstageFiles(repoId, staged.map((f) => f.path), worktreePath), "Unstaged all"),
                false,
              )}
              {staged.map((f) => renderFile(f, true))}
            </>
          )}
          {unstaged.length > 0 && (
            <>
              {sectionHeader(
                "all:unstaged",
                "Unstaged",
                "Stage all",
                () => act(stageFiles(repoId, unstaged.map((f) => f.path), worktreePath), "Staged all"),
                staged.length > 0,
              )}
              {unstaged.map((f) => renderFile(f, false))}
            </>
          )}
        </div>
      )}
    </div>
  );
}

export function CommitGraph({
  repoId,
  worktreePath,
  refs,
  pageSize,
  refreshKey,
  confirmActions,
  theme,
  onToast,
  onChanged,
}: {
  repoId: string;
  worktreePath?: string;
  refs: string[];
  pageSize: number;
  refreshKey: number;
  confirmActions: boolean;
  theme: ThemeName;
  onToast: (msg: string) => void;
  onChanged: () => void;
}) {
  const [commits, setCommits] = useState<CommitInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [done, setDone] = useState(false);
  const [scrollTop, setScrollTop] = useState(0);
  const [viewH, setViewH] = useState(400);
  const [expandedHash, setExpandedHash] = useState<string | null>(null);
  const [detail, setDetail] = useState<CommitDetail | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; commit: CommitInfo } | null>(null);
  const [prompt, setPrompt] = useState<{ x: number; y: number; hash: string } | null>(null);

  const containerRef = useRef<HTMLDivElement>(null);
  const loadingMoreRef = useRef(false);
  const expandedHashRef = useRef<string | null>(null);
  expandedHashRef.current = expandedHash;
  const lenRef = useRef(0);
  lenRef.current = commits.length;

  // Array identity changes each render; a joined key drives the reload effect. \n can't appear in a ref name.
  const refsKey = refs.join("\n");

  // Initial load / branch or repo switch: reset everything.
  useEffect(() => {
    let live = true;
    setCommits([]);
    setDone(false);
    setLoading(true);
    setExpandedHash(null);
    setDetail(null);
    setScrollTop(0);
    containerRef.current?.scrollTo(0, 0);
    getLog(repoId, refs, 0, pageSize)
      .then((cs) => {
        if (!live) return;
        setCommits(cs);
        setDone(cs.length < pageSize);
      })
      .catch((e: unknown) => onToast(String(e)))
      .finally(() => {
        if (live) setLoading(false);
      });
    return () => {
      live = false;
    };
  }, [repoId, refsKey, pageSize]); // eslint-disable-line react-hooks/exhaustive-deps

  // External refresh (repo-changed, branch ops): reload the loaded window in place.
  useEffect(() => {
    if (refreshKey === 0) return;
    const limit = Math.max(lenRef.current, pageSize);
    let live = true;
    getLog(repoId, refs, 0, limit)
      .then((cs) => {
        if (!live) return;
        setCommits(cs);
        setDone(cs.length < limit);
      })
      .catch((e: unknown) => onToast(String(e)));
    return () => {
      live = false;
    };
  }, [refreshKey]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setViewH(el.clientHeight));
    ro.observe(el);
    setViewH(el.clientHeight);
    return () => ro.disconnect();
  }, []);

  const { rows, maxLanes } = useMemo(() => layoutGraph(commits), [commits]);
  const railW = Math.min(maxLanes, MAX_RAIL_LANES) * LANE_W + 4;

  const loadMore = () => {
    if (loadingMoreRef.current || done || loading) return;
    loadingMoreRef.current = true;
    getLog(repoId, refs, lenRef.current, pageSize)
      .then((next) => {
        if (next.length < pageSize) setDone(true);
        setCommits((cur) => [...cur, ...next]);
      })
      .catch((e: unknown) => onToast(String(e)))
      .finally(() => {
        loadingMoreRef.current = false;
      });
  };

  const onScroll = (e: UIEvent<HTMLDivElement>) => {
    const el = e.currentTarget;
    setScrollTop(el.scrollTop);
    if (el.scrollTop + el.clientHeight > el.scrollHeight - ROW_H * 12) loadMore();
  };

  const toggleExpand = (c: CommitInfo) => {
    if (expandedHash === c.hash) {
      setExpandedHash(null);
      setDetail(null);
      return;
    }
    setExpandedHash(c.hash);
    setDetail(null);
    getCommit(repoId, c.hash)
      .then((d) => {
        if (expandedHashRef.current === d.hash) setDetail(d);
      })
      .catch((e: unknown) => {
        onToast(String(e));
        if (expandedHashRef.current === c.hash) setExpandedHash(null);
      });
  };

  const openMenu = (e: MouseEvent, c: CommitInfo) => {
    e.preventDefault();
    setMenu({ x: e.clientX, y: e.clientY, commit: c });
  };

  const copy = (text: string, ok: string) => {
    void navigator.clipboard.writeText(text);
    onToast(ok);
  };

  const menuItems = (m: { x: number; y: number; commit: CommitInfo }): MenuItem[] => [
    { label: "Copy hash", onClick: () => copy(m.commit.hash, "Hash copied") },
    { label: "Copy message", onClick: () => copy(m.commit.subject, "Message copied") },
    {
      label: "Create branch here…",
      onClick: () => setPrompt({ x: m.x, y: m.y, hash: m.commit.hash }),
    },
    {
      label: "Checkout (detached)",
      onClick: () =>
        checkout(repoId, m.commit.hash, worktreePath)
          .then(() => {
            onToast(`Checked out ${m.commit.hash.slice(0, 7)}`);
            onChanged();
          })
          .catch((e: unknown) => onToast(String(e))),
    },
  ];

  // ---- virtualization math (single expandable detail panel shifts rows below it)
  const expIdx = expandedHash === null ? -1 : commits.findIndex((c) => c.hash === expandedHash);
  const detailOffset = expIdx >= 0 ? DETAIL_H : 0;
  const totalH = commits.length * ROW_H + detailOffset + (done ? 0 : ROW_H);
  const rowTop = (i: number) => i * ROW_H + (expIdx >= 0 && i > expIdx ? DETAIL_H : 0);
  const first = Math.max(0, Math.floor((scrollTop - detailOffset) / ROW_H) - OVERSCAN);
  const last = Math.min(commits.length - 1, Math.ceil((scrollTop + viewH) / ROW_H) + OVERSCAN);

  const visible: ReactNode[] = [];
  for (let i = first; i <= last; i++) {
    const c = commits[i];
    const row = rows[i];
    if (!c || !row) break;
    const isExpanded = i === expIdx;
    const pills = c.refs.slice(0, 2);
    const extraPills = c.refs.length - pills.length;
    visible.push(
      <div
        key={c.hash}
        className={`absolute inset-x-0 flex cursor-default items-center gap-1.5 pr-2 ${
          isExpanded ? "bg-sel" : "hover:bg-hover"
        }`}
        style={{ top: rowTop(i), height: ROW_H }}
        onClick={() => toggleExpand(c)}
        onContextMenu={(e) => openMenu(e, c)}
      >
        <Rail row={row} railW={railW} theme={theme} />
        <span className="min-w-0 flex-1 truncate text-[12px]" title={c.subject}>
          {c.subject}
        </span>
        {pills.map((r) => (
          <RefPill key={r} r={r} theme={theme} />
        ))}
        {extraPills > 0 && (
          <span className="group/refs relative shrink-0 cursor-help rounded-sm bg-panel2 px-1 text-[9px] leading-[14px] text-faint transition-colors hover:text-fg">
            +{extraPills}
            <span className="pointer-events-none absolute top-full right-0 z-20 mt-1 hidden min-w-max flex-col items-start gap-1 rounded-md border border-edge bg-panel p-1.5 shadow-xl shadow-black/20 group-hover/refs:flex">
              {c.refs.slice(2).map((r) => (
                <RefPill key={r} r={r} theme={theme} full />
              ))}
            </span>
          </span>
        )}
        <span className="shrink-0 font-mono text-[10px] text-faint tabular-nums">
          {c.hash.slice(0, 7)}
        </span>
        <span className="w-[26px] shrink-0 text-right text-[10px] whitespace-nowrap text-faint tabular-nums">
          {relTime(c.timestamp)}
        </span>
      </div>,
    );
  }

  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <WorkingChanges
        repoId={repoId}
        worktreePath={worktreePath}
        refreshKey={refreshKey}
        confirmActions={confirmActions}
        onToast={onToast}
        onChanged={onChanged}
      />
      <div className="relative min-h-0 flex-1">
      {/* Container stays mounted across empty/loading states so the ResizeObserver keeps tracking. */}
      <div ref={containerRef} className="h-full overflow-y-auto" onScroll={onScroll}>
        {commits.length === 0 ? (
          <div className="flex h-full items-center justify-center text-[12px] text-faint">
            {loading ? "Loading commits…" : "No commits"}
          </div>
        ) : (
          <div className="relative" style={{ height: totalH }}>
            {visible}
            {expIdx >= 0 && (
              <DetailPanel
                top={rowTop(expIdx) + ROW_H}
                repoId={repoId}
                detail={detail}
                onToast={onToast}
              />
            )}
            {!done && (
              <div
                className="absolute inset-x-0 flex items-center justify-center text-[11px] text-faint"
                style={{ top: totalH - ROW_H, height: ROW_H }}
              >
                Loading more…
              </div>
            )}
          </div>
        )}
      </div>

      {menu && (
        <ContextMenu x={menu.x} y={menu.y} items={menuItems(menu)} onClose={() => setMenu(null)} />
      )}
      {prompt && (
        <PromptPopover
          x={prompt.x}
          y={prompt.y}
          title={`New branch at ${prompt.hash.slice(0, 7)}`}
          withInput
          placeholder="branch name"
          confirmLabel="Create"
          onConfirm={async (v) => {
            await createBranch(repoId, v, prompt.hash);
            onToast(`Created ${v}`);
            onChanged();
          }}
          onClose={() => setPrompt(null)}
        />
      )}
      </div>
    </div>
  );
}
