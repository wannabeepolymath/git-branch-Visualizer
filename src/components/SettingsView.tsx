// ponytail: placeholder — a later phase replaces this routed view with real settings.
export function SettingsView({ onBack }: { onBack: () => void }) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-3">
      <div className="text-[13px] font-medium text-neutral-500 dark:text-neutral-400">
        Settings coming soon
      </div>
      <button
        className="rounded border border-neutral-300 px-3 py-1 text-[12px] text-neutral-600 hover:bg-neutral-100 dark:border-neutral-600 dark:text-neutral-300 dark:hover:bg-neutral-800"
        onClick={onBack}
      >
        Back
      </button>
    </div>
  );
}
