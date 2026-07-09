import { useEffect, useState, type KeyboardEvent } from "react";
import { getSettings, removeRepo, updateSettings, type RepoInfo, type Settings } from "../lib/ipc";
import { PromptPopover } from "./ContextMenu";

const MODIFIER_KEYS = new Set(["Control", "Alt", "Shift", "Meta"]);

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
    <div className="px-3 pt-3 pb-1 text-[10px] font-semibold tracking-wider text-neutral-400 uppercase">
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
  onBack,
  onAddRepo,
  onToast,
}: {
  settings: Settings;
  onSettingsChange: (s: Settings) => void;
  onBack: () => void;
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
            <div className="px-3 py-1 text-[11px] text-neutral-400">No repositories yet</div>
          )}
          {settings.repos.map((r) => (
            <div key={r.id} className="flex h-8 items-center gap-2 px-3">
              <div className="min-w-0 flex-1">
                <div
                  className={`truncate text-[12px] ${
                    r.id === settings.activeRepoId
                      ? "font-semibold text-blue-600 dark:text-blue-400"
                      : "text-neutral-800 dark:text-neutral-200"
                  }`}
                >
                  {r.name}
                </div>
                <div className="truncate text-[10px] text-neutral-400">{r.path}</div>
              </div>
              <button
                aria-label={`Remove ${r.name}`}
                className="flex size-5 shrink-0 items-center justify-center rounded text-neutral-400 hover:bg-neutral-100 hover:text-red-600 dark:hover:bg-neutral-800 dark:hover:text-red-400"
                onClick={(e) => setRemoveTarget({ x: e.clientX, y: e.clientY, repo: r })}
              >
                <XIcon />
              </button>
            </div>
          ))}
        </div>
        <div className="px-3 pt-2">
          <button
            className="rounded border border-neutral-300 px-2 py-1 text-[11px] text-neutral-600 hover:bg-neutral-100 dark:border-neutral-600 dark:text-neutral-300 dark:hover:bg-neutral-800"
            onClick={onAddRepo}
          >
            Add repository…
          </button>
        </div>

        <SectionLabel>Shortcut</SectionLabel>
        <div className="px-3">
          <label className="mb-1 block text-[11px] text-neutral-500 dark:text-neutral-400" htmlFor="shortcut-recorder">
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
              recording
                ? "border-blue-500 text-blue-600 dark:text-blue-400"
                : "border-neutral-300 text-neutral-800 dark:border-neutral-600 dark:text-neutral-200"
            } bg-white dark:bg-neutral-900`}
          >
            {recording ? "Press a key combination… (Esc to cancel)" : settings.shortcut}
          </button>
        </div>

        <SectionLabel>General</SectionLabel>
        <label className="flex h-8 items-center gap-2 px-3 text-[12px]">
          <input
            type="checkbox"
            className="size-3.5 accent-blue-600"
            checked={settings.launchAtLogin}
            onChange={(e) => void patch({ launchAtLogin: e.target.checked })}
          />
          Launch at login
        </label>
        <label className="flex h-8 items-center gap-2 px-3 text-[12px]">
          <input
            type="checkbox"
            className="size-3.5 accent-blue-600"
            checked={settings.confirmActions}
            onChange={(e) => void patch({ confirmActions: e.target.checked })}
          />
          Confirm before staging, discarding, and other file actions
        </label>
        <div className="flex items-center gap-2 px-3 py-1">
          <label className="text-[12px] text-neutral-600 dark:text-neutral-300" htmlFor="theme-select">
            Theme
          </label>
          <select
            id="theme-select"
            value={settings.theme}
            onChange={(e) => void patch({ theme: e.target.value as Settings["theme"] })}
            className="rounded border border-neutral-300 bg-white px-1.5 py-0.5 text-[12px] outline-none dark:border-neutral-600 dark:bg-neutral-900"
          >
            <option value="system">System</option>
            <option value="light">Light</option>
            <option value="dark">Dark</option>
          </select>
        </div>

        <SectionLabel>Graph</SectionLabel>
        <div className="flex items-center gap-2 px-3 py-1">
          <label className="text-[12px] text-neutral-600 dark:text-neutral-300" htmlFor="commits-per-page">
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
            className="w-20 rounded border border-neutral-300 bg-white px-1.5 py-0.5 text-[12px] outline-none dark:border-neutral-600 dark:bg-neutral-900"
          />
        </div>
        <label className="flex h-8 items-center gap-2 px-3 text-[12px]">
          <input
            type="checkbox"
            className="size-3.5 accent-blue-600"
            checked={settings.showRemoteBranches}
            onChange={(e) => void patch({ showRemoteBranches: e.target.checked })}
          />
          Show remote branches by default
        </label>
      </div>

      <div className="border-t border-neutral-200 p-2 dark:border-neutral-800">
        <button
          className="rounded border border-neutral-300 px-3 py-1 text-[12px] text-neutral-600 hover:bg-neutral-100 dark:border-neutral-600 dark:text-neutral-300 dark:hover:bg-neutral-800"
          onClick={onBack}
        >
          Back
        </button>
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
