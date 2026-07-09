import { useEffect, useState, type KeyboardEvent } from "react";
import {
  getSettings,
  normalizeTheme,
  removeRepo,
  THEME_NAMES,
  updateSettings,
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

function SectionLabel({ children }: { children: string }) {
  return (
    <div className="px-3 pt-3 pb-1 text-[10px] font-semibold tracking-wider text-faint uppercase">
      {children}
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
  const [cppText, setCppText] = useState(String(settings.commitsPerPage));
  useEffect(() => setCppText(String(settings.commitsPerPage)), [settings.commitsPerPage]);

  const patch = (next: Partial<Settings>) =>
    updateSettings({ ...settings, ...next })
      .then(onSettingsChange)
      .catch((e: unknown) => onToast(String(e)));

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
        <SectionLabel>Repositories</SectionLabel>
        <div>
          {settings.repos.length === 0 && (
            <div className="px-3 py-1 text-[11px] text-faint">No repositories yet</div>
          )}
          {settings.repos.map((r) => (
            <div key={r.id} className="flex h-8 items-center gap-2 px-3">
              <div className="min-w-0 flex-1">
                <div
                  className={`truncate text-[12px] ${
                    r.id === settings.activeRepoId
                      ? "font-semibold text-accent"
                      : "text-fg"
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

        <SectionLabel>Shortcut</SectionLabel>
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

        <SectionLabel>General</SectionLabel>
        <label className="flex h-8 items-center gap-2 px-3 text-[12px]">
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
        <SectionLabel>Theme</SectionLabel>
        <div className="grid grid-cols-3 gap-2 px-3 pt-1">
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

        <SectionLabel>Graph</SectionLabel>
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
