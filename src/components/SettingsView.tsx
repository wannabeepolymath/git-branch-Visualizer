import { useEffect, useState, type KeyboardEvent, type ReactNode } from "react";
import {
  getSettings,
  normalizeTheme,
  removeRepo,
  THEME_NAMES,
  updateSettings,
  type OpenTarget,
  type RepoInfo,
  type Settings,
  type ThemeName,
} from "../lib/ipc";
import { PromptPopover } from "./ContextMenu";

const MODIFIER_KEYS = new Set(["Control", "Alt", "Shift", "Meta"]);

// Name + one-line blurb + a few representative colors (panel, edge, accent, lane)
// for each theme's picker card. Kept in sync with the palettes in index.css.
const THEME_META: Record<ThemeName, { label: string; blurb: string; swatch: [string, string, string, string] }> = {
  graphite: { label: "Graphite", blurb: "Calm, dark, pro", swatch: ["#12141b", "#23262f", "#8b8cf6", "#48b4c4"] },
  paper: { label: "Paper", blurb: "Light, roomy", swatch: ["#ffffff", "#e4e8e7", "#0f8a72", "#4f46e5"] },
  terminal: { label: "Terminal", blurb: "Dark, mono", swatch: ["#0a0f0c", "#17271c", "#4ade80", "#22d3ee"] },
};

/** Tauri global-shortcut plugin format: modifiers joined by "+", then the key. */
function formatShortcut(e: KeyboardEvent): string | null {
  const mods: string[] = [];
  if (e.ctrlKey) mods.push("Ctrl");
  if (e.altKey) mods.push("Alt");
  if (e.shiftKey) mods.push("Shift");
  if (e.metaKey) mods.push("Super");
  if (mods.length === 0) return null;
  // Letters/digits come from e.code: with Option held, macOS remaps e.key to
  // symbols ("©"), which the global-shortcut plugin can't parse.
  const codeKey =
    e.code.startsWith("Key") && e.code.length === 4
      ? e.code.slice(3)
      : e.code.startsWith("Digit") && e.code.length === 6
        ? e.code.slice(5)
        : null;
  const key = codeKey ?? (e.key.length === 1 ? e.key.toUpperCase() : e.key);
  return [...mods, key].join("+");
}

const SECTION_IDS = ["repos", "general", "worktrees", "appearance", "graph", "commands"] as const;
type SectionId = (typeof SECTION_IDS)[number];

// Read-only reference of the git command each button runs. Mirrors the ops in
// src-tauri/src/git.rs — keep in sync when a command's flags change there.
// Every command also runs with `git -C <repo> --no-optional-locks` (omitted for clarity).
const COMMAND_REF: { action: string; cmd: string }[] = [
  { action: "Stage", cmd: "git add -- <paths>" },
  { action: "Unstage", cmd: "git restore --staged -- <paths>" },
  { action: "Discard changes", cmd: "git restore -- <paths>" },
  { action: "Discard untracked", cmd: "git clean -f -- <paths>" },
  { action: "Switch / checkout", cmd: "git checkout <ref>" },
  { action: "New branch", cmd: "git branch <name> [<from>]" },
  { action: "Rename branch", cmd: "git branch -m <old> <new>" },
  { action: "Delete branch", cmd: "git branch -d <name>" },
  { action: "Fetch", cmd: "git fetch --all --prune" },
  { action: "Pull", cmd: "git pull --ff-only" },
  { action: "Publish branch", cmd: "git push --set-upstream origin <branch>" },
  { action: "Push", cmd: "git push origin <branch>" },
  { action: "Force push", cmd: "git push --force-with-lease origin <branch>" },
  { action: "Open worktree (↗)", cmd: "your selected Worktrees command" },
];

const OPEN_STORAGE_KEY = "bv.settingsOpen";

/** Read the persisted open-set. First run (nothing stored) → only Repositories. */
function loadOpenSections(): Set<SectionId> {
  const raw = localStorage.getItem(OPEN_STORAGE_KEY);
  if (raw === null) return new Set<SectionId>(["repos"]);
  return new Set(
    raw.split(",").filter((id): id is SectionId => (SECTION_IDS as readonly string[]).includes(id)),
  );
}

function ChevronIcon({ open }: { open: boolean }) {
  return (
    <svg
      width="9"
      height="9"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="3"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={`shrink-0 text-faint transition-transform ${open ? "rotate-90" : ""}`}
    >
      <path d="m9 18 6-6-6-6" />
    </svg>
  );
}

function Section({
  title,
  badge,
  open,
  onToggle,
  children,
}: {
  title: string;
  badge?: number;
  open: boolean;
  onToggle: () => void;
  children: ReactNode;
}) {
  return (
    <div>
      <button
        type="button"
        aria-expanded={open}
        onClick={onToggle}
        className="flex w-full items-center gap-1.5 px-3 py-2 text-left hover:bg-hover"
      >
        <ChevronIcon open={open} />
        <span className="text-[10px] font-semibold tracking-wider text-faint uppercase">{title}</span>
        {badge !== undefined && <span className="text-[10px] text-faint">({badge})</span>}
      </button>
      {open && <div className="pb-2">{children}</div>}
    </div>
  );
}

function XIcon() {
  return (
    <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M18 6 6 18M6 6l12 12" />
    </svg>
  );
}

export function SettingsView({
  settings,
  onSettingsChange,
  onAddRepo,
  onToast,
}: {
  settings: Settings;
  onSettingsChange: (s: Settings) => void;
  onAddRepo: () => void;
  onToast: (msg: string) => void;
}) {
  const [removeTarget, setRemoveTarget] = useState<{ x: number; y: number; repo: RepoInfo } | null>(
    null,
  );
  const [recording, setRecording] = useState(false);
  const [openSections, setOpenSections] = useState<Set<SectionId>>(loadOpenSections);
  const toggleSection = (id: SectionId) =>
    setOpenSections((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      localStorage.setItem(OPEN_STORAGE_KEY, [...next].join(","));
      return next;
    });
  const [cppText, setCppText] = useState(String(settings.commitsPerPage));
  useEffect(() => setCppText(String(settings.commitsPerPage)), [settings.commitsPerPage]);
  // Local copy for smooth typing; persisted on blur (like commits-per-page).
  const [targets, setTargets] = useState<OpenTarget[]>(settings.openTargets);
  useEffect(() => setTargets(settings.openTargets), [settings.openTargets]);

  const patch = (next: Partial<Settings>) =>
    updateSettings({ ...settings, ...next })
      .then(onSettingsChange)
      .catch((e: unknown) => onToast(String(e)));

  const editTarget = (i: number, next: Partial<OpenTarget>) =>
    setTargets((ts) => ts.map((t, j) => (j === i ? { ...t, ...next } : t)));

  const commitTargets = () => {
    if (JSON.stringify(targets) !== JSON.stringify(settings.openTargets)) {
      void patch({ openTargets: targets });
    }
  };

  const addTarget = () =>
    void patch({
      openTargets: [
        ...settings.openTargets,
        { id: crypto.randomUUID(), name: "New", command: "code {path}" },
      ],
    });

  const removeOpenTarget = (id: string) => {
    const next = settings.openTargets.filter((t) => t.id !== id);
    void patch({
      openTargets: next,
      // Keep the default pointing at something real.
      defaultOpenTarget:
        settings.defaultOpenTarget === id ? (next[0]?.id ?? null) : settings.defaultOpenTarget,
    });
  };

  const commitCommitsPerPage = () => {
    const parsed = Math.round(Number(cppText));
    const n = Number.isFinite(parsed) ? Math.min(1000, Math.max(50, parsed)) : settings.commitsPerPage;
    setCppText(String(n));
    if (n !== settings.commitsPerPage) void patch({ commitsPerPage: n });
  };

  const onShortcutKeyDown = (e: KeyboardEvent<HTMLButtonElement>) => {
    if (!recording) return;
    e.preventDefault();
    if (e.key === "Escape") {
      setRecording(false);
      e.currentTarget.blur();
      return;
    }
    if (MODIFIER_KEYS.has(e.key)) return; // waiting for a non-modifier key
    const combo = formatShortcut(e);
    if (!combo) return; // no modifier held yet — keep recording
    setRecording(false);
    e.currentTarget.blur();
    void patch({ shortcut: combo });
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto pb-3">
        <Section
          title="Repositories"
          badge={settings.repos.length}
          open={openSections.has("repos")}
          onToggle={() => toggleSection("repos")}
        >
          <div>
            {settings.repos.length === 0 && (
              <div className="px-3 py-1 text-[11px] text-faint">No repositories yet</div>
            )}
            {settings.repos.map((r) => (
              <div key={r.id} className="flex h-8 items-center gap-2 px-3">
                <div className="min-w-0 flex-1">
                  <div
                    className={`truncate text-[12px] ${
                      r.id === settings.activeRepoId ? "font-semibold text-accent" : "text-fg"
                    }`}
                  >
                    {r.name}
                  </div>
                  <div className="truncate text-[10px] text-faint">{r.path}</div>
                </div>
                <button
                  aria-label={`Remove ${r.name}`}
                  className="flex size-5 shrink-0 items-center justify-center rounded text-faint hover:bg-hover hover:text-del"
                  onClick={(e) => setRemoveTarget({ x: e.clientX, y: e.clientY, repo: r })}
                >
                  <XIcon />
                </button>
              </div>
            ))}
          </div>
          <div className="px-3 pt-2">
            <button
              className="rounded border border-edge px-2 py-1 text-[11px] text-muted hover:bg-hover"
              onClick={onAddRepo}
            >
              Add repository…
            </button>
          </div>
        </Section>

        <Section
          title="General"
          open={openSections.has("general")}
          onToggle={() => toggleSection("general")}
        >
          <div className="px-3">
            <label className="mb-1 block text-[11px] text-muted" htmlFor="shortcut-recorder">
              Global shortcut
            </label>
            <button
              id="shortcut-recorder"
              type="button"
              aria-label="Global shortcut recorder. Focus and press a key combination to set it."
              onFocus={() => setRecording(true)}
              onBlur={() => setRecording(false)}
              onKeyDown={onShortcutKeyDown}
              className={`w-full rounded border px-2 py-1 text-left text-[12px] outline-none ${
                recording ? "border-accent text-accent" : "border-edge text-fg"
              } bg-surface`}
            >
              {recording ? "Press a key combination… (Esc to cancel)" : settings.shortcut}
            </button>
          </div>
          <label className="mt-1 flex h-8 items-center gap-2 px-3 text-[12px]">
            <input
              type="checkbox"
              className="size-3.5 accent-accent"
              checked={settings.launchAtLogin}
              onChange={(e) => void patch({ launchAtLogin: e.target.checked })}
            />
            Launch at login
          </label>
          <label className="flex h-8 items-center gap-2 px-3 text-[12px]">
            <input
              type="checkbox"
              className="size-3.5 accent-accent"
              checked={settings.confirmActions}
              onChange={(e) => void patch({ confirmActions: e.target.checked })}
            />
            Confirm before staging, discarding, and other file actions
          </label>
        </Section>

        <Section
          title="Worktrees"
          badge={settings.openTargets.length}
          open={openSections.has("worktrees")}
          onToggle={() => toggleSection("worktrees")}
        >
          <div className="px-3">
            <p className="mb-1.5 text-[10px] text-faint">
              Commands to open a worktree. <span className="text-muted">{"{path}"}</span> is replaced
              with the worktree folder; the selected default is used by the ↗ button.
            </p>
            <div className="flex flex-col gap-1.5">
              {targets.map((t, i) => (
                <div key={t.id} className="flex items-center gap-1.5">
                  <input
                    type="radio"
                    name="default-open-target"
                    className="size-3.5 shrink-0 accent-accent"
                    checked={settings.defaultOpenTarget === t.id}
                    onChange={() => void patch({ defaultOpenTarget: t.id })}
                    title="Use as the default (↗) target"
                  />
                  <input
                    value={t.name}
                    placeholder="Name"
                    aria-label="Target name"
                    onChange={(e) => editTarget(i, { name: e.target.value })}
                    onBlur={commitTargets}
                    className="w-20 shrink-0 rounded border border-edge bg-surface px-1.5 py-0.5 text-[12px] outline-none focus:border-accent"
                  />
                  <input
                    value={t.command}
                    placeholder="code {path}"
                    aria-label="Target command"
                    onChange={(e) => editTarget(i, { command: e.target.value })}
                    onBlur={commitTargets}
                    className="min-w-0 flex-1 rounded border border-edge bg-surface px-1.5 py-0.5 font-mono text-[11px] outline-none focus:border-accent"
                  />
                  <button
                    aria-label={`Remove ${t.name}`}
                    className="flex size-5 shrink-0 items-center justify-center rounded text-faint hover:bg-hover hover:text-del"
                    onClick={() => removeOpenTarget(t.id)}
                  >
                    <XIcon />
                  </button>
                </div>
              ))}
            </div>
            <button
              className="mt-2 rounded border border-edge px-2 py-1 text-[11px] text-muted hover:bg-hover"
              onClick={addTarget}
            >
              Add target…
            </button>
          </div>
        </Section>

        <Section
          title="Appearance"
          open={openSections.has("appearance")}
          onToggle={() => toggleSection("appearance")}
        >
          <div className="grid grid-cols-3 gap-2 px-3">
            {THEME_NAMES.map((t) => {
              const m = THEME_META[t];
              const active = normalizeTheme(settings.theme) === t;
              return (
                <button
                  key={t}
                  aria-pressed={active}
                  onClick={() => void patch({ theme: t })}
                  className={`overflow-hidden rounded-md border text-left ${
                    active ? "border-accent ring-1 ring-accent" : "border-edge hover:border-muted"
                  }`}
                >
                  <div className="flex h-8 items-center justify-center gap-1" style={{ background: m.swatch[0] }}>
                    <span className="size-2.5 rounded-full" style={{ background: m.swatch[2] }} />
                    <span className="size-2.5 rounded-full" style={{ background: m.swatch[3] }} />
                    <span className="size-2.5 rounded-full" style={{ background: m.swatch[1] }} />
                  </div>
                  <div className="px-2 py-1.5">
                    <div className="flex items-center gap-1 text-[12px] font-medium text-fg">
                      {m.label}
                      {active && <span className="text-accent">✓</span>}
                    </div>
                    <div className="text-[10px] text-faint">{m.blurb}</div>
                  </div>
                </button>
              );
            })}
          </div>
        </Section>

        <Section
          title="Graph"
          open={openSections.has("graph")}
          onToggle={() => toggleSection("graph")}
        >
          <div className="flex items-center gap-2 px-3 py-1">
            <label className="text-[12px] text-muted" htmlFor="commits-per-page">
              Commits per page
            </label>
            <input
              id="commits-per-page"
              type="number"
              min={50}
              max={1000}
              value={cppText}
              onChange={(e) => setCppText(e.target.value)}
              onBlur={commitCommitsPerPage}
              onKeyDown={(e) => {
                if (e.key === "Enter") e.currentTarget.blur();
              }}
              className="w-20 rounded border border-edge bg-surface px-1.5 py-0.5 text-[12px] outline-none"
            />
          </div>
          <label className="flex h-8 items-center gap-2 px-3 text-[12px]">
            <input
              type="checkbox"
              className="size-3.5 accent-accent"
              checked={settings.showRemoteBranches}
              onChange={(e) => void patch({ showRemoteBranches: e.target.checked })}
            />
            Show remote branches by default
          </label>
        </Section>

        <Section
          title="Commands"
          badge={COMMAND_REF.length}
          open={openSections.has("commands")}
          onToggle={() => toggleSection("commands")}
        >
          <p className="mb-1.5 px-3 text-[10px] text-faint">
            The git command each button runs, for reference. Read-only; runs in the active repo or
            focused worktree.
          </p>
          <div className="flex flex-col gap-1.5 px-3">
            {COMMAND_REF.map((c) => (
              <div key={c.action} className="flex flex-col gap-0.5">
                <span className="text-[11px] text-muted">{c.action}</span>
                <code className="block rounded border border-edge bg-surface px-1.5 py-0.5 font-mono text-[11px] break-all text-fg select-text">
                  {c.cmd}
                </code>
              </div>
            ))}
          </div>
        </Section>
      </div>

      {removeTarget && (
        <PromptPopover
          x={removeTarget.x}
          y={removeTarget.y}
          title={`Remove ${removeTarget.repo.name}?`}
          confirmLabel="Remove"
          danger
          onClose={() => setRemoveTarget(null)}
          onConfirm={async () => {
            await removeRepo(removeTarget.repo.id);
            onToast(`Removed ${removeTarget.repo.name}`);
            onSettingsChange(await getSettings());
          }}
        />
      )}
    </div>
  );
}
