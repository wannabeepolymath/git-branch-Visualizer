import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type ReactNode,
  type RefObject,
} from "react";

export interface MenuItem {
  label: string;
  danger?: boolean;
  onClick: () => void;
}

/** Close on Escape or on mousedown outside `ref`. */
export function useDismiss(ref: RefObject<HTMLElement | null>, onClose: () => void): void {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    const onDown = (e: MouseEvent) => {
      if (ref.current && e.target instanceof Node && !ref.current.contains(e.target)) onClose();
    };
    window.addEventListener("keydown", onKey);
    window.addEventListener("mousedown", onDown);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("mousedown", onDown);
    };
  }, [ref, onClose]);
}

/** Fixed-position popup clamped to the viewport, dismissed on click-away/Escape. */
function Popup({
  x,
  y,
  w,
  onClose,
  children,
}: {
  x: number;
  y: number;
  w: number;
  onClose: () => void;
  children: ReactNode;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [h, setH] = useState(0);
  useDismiss(ref, onClose);
  useLayoutEffect(() => {
    if (ref.current) setH(ref.current.offsetHeight);
  }, [children]);
  const left = Math.max(4, Math.min(x, window.innerWidth - w - 4));
  const top = Math.max(4, Math.min(y, window.innerHeight - h - 4));
  return (
    <div
      ref={ref}
      className="fixed z-50 rounded-md border border-neutral-200 bg-white shadow-xl shadow-black/10 dark:border-neutral-700 dark:bg-neutral-800 dark:shadow-black/40"
      style={{ left, top, width: w }}
    >
      {children}
    </div>
  );
}

export function ContextMenu({
  x,
  y,
  items,
  onClose,
}: {
  x: number;
  y: number;
  items: MenuItem[];
  onClose: () => void;
}) {
  return (
    <Popup x={x} y={y} w={192} onClose={onClose}>
      <div className="py-1">
        {items.map((it) => (
          <button
            key={it.label}
            className={`block w-full px-3 py-1 text-left text-[12px] hover:bg-neutral-100 dark:hover:bg-neutral-700 ${
              it.danger ? "text-red-600 dark:text-red-400" : "text-neutral-800 dark:text-neutral-200"
            }`}
            onClick={() => {
              onClose();
              it.onClick();
            }}
          >
            {it.label}
          </button>
        ))}
      </div>
    </Popup>
  );
}

/**
 * Inline prompt/confirm popover. `onConfirm` may reject — the error string is
 * shown inside the popover (e.g. "branch not merged" from delete_branch).
 */
export function PromptPopover({
  x,
  y,
  title,
  withInput = false,
  initial = "",
  placeholder,
  confirmLabel,
  danger = false,
  onConfirm,
  onClose,
}: {
  x: number;
  y: number;
  title: string;
  withInput?: boolean;
  initial?: string;
  placeholder?: string;
  confirmLabel: string;
  danger?: boolean;
  onConfirm: (value: string) => Promise<void>;
  onClose: () => void;
}) {
  const [value, setValue] = useState(initial);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const submit = () => {
    const v = value.trim();
    if (withInput && !v) {
      setError("Name is required");
      return;
    }
    setBusy(true);
    setError(null);
    onConfirm(v)
      .then(onClose)
      .catch((e: unknown) => {
        setError(String(e));
        setBusy(false);
      });
  };

  return (
    <Popup x={x} y={y} w={236} onClose={onClose}>
      <div className="p-2.5">
        <div className="mb-2 text-[12px] font-medium">{title}</div>
        {withInput && (
          <input
            autoFocus
            value={value}
            placeholder={placeholder}
            onChange={(e) => setValue(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") submit();
            }}
            className="mb-2 w-full rounded border border-neutral-300 bg-white px-2 py-1 text-[12px] text-neutral-900 outline-none placeholder:text-neutral-400 focus:border-blue-500 dark:border-neutral-600 dark:bg-neutral-900 dark:text-neutral-100"
          />
        )}
        {error && (
          <div className="mb-2 text-[11px] whitespace-pre-wrap text-red-600 select-text dark:text-red-400">
            {error}
          </div>
        )}
        <div className="flex justify-end gap-1.5">
          <button
            className="rounded px-2 py-1 text-[11px] text-neutral-600 hover:bg-neutral-100 dark:text-neutral-300 dark:hover:bg-neutral-700"
            onClick={onClose}
          >
            Cancel
          </button>
          <button
            disabled={busy}
            className={`rounded px-2 py-1 text-[11px] font-medium text-white disabled:opacity-50 ${
              danger ? "bg-red-600 hover:bg-red-500" : "bg-blue-600 hover:bg-blue-500"
            }`}
            onClick={submit}
          >
            {busy ? "Working…" : confirmLabel}
          </button>
        </div>
      </div>
    </Popup>
  );
}
