# Settings: Accordion Redesign

**Date:** 2026-07-10
**Component:** `src/components/SettingsView.tsx`

## Problem

`SettingsView` renders all sections stacked in one long vertical scroll,
separated only by small uppercase labels. In the 420×560 window (min 320×400)
this reads as cluttered and gives no way to navigate — reaching the last
section means scrolling past every other one, with every control visible and
competing at once.

## Goal

Make settings easy to scan and navigate without adding chrome the narrow
window can't hold. Keep the change small: wrap existing sections, don't rewrite
the controls inside them.

## Design

### Layout: collapsible accordion

Introduce one small `Section` component **inside `SettingsView.tsx`** (no new
file) that replaces the current `SectionLabel` + block pattern:

```tsx
<Section id="repos" title="Repositories" badge={settings.repos.length}>
  {/* existing repo-list markup, unchanged */}
</Section>
```

- **Header row:** chevron (`▾` open / `▸` closed) + title + optional count
  badge, full-width and clickable, `hover:bg-hover`. Replaces `SectionLabel`.
- **Body:** the existing per-section markup, rendered only when the section is
  open. No control logic changes.

`SectionLabel` is removed (superseded by `Section`'s header). `XIcon`,
`formatShortcut`, `THEME_META`, and all handlers stay as-is.

### Behavior

- **Independent toggles** — each section opens/closes on its own; opening one
  does not close others.
- **Persisted open-set** — the set of open section ids is stored in
  `localStorage` (key `settings.openSections`), consistent with how the branch
  panel already persists its state. On first run (nothing stored) only
  `Repositories` is open; all others collapsed.
- Persistence read once on mount into state; each toggle updates both state and
  `localStorage`.

### Sections (6 → 5)

| id       | Title          | Badge            | Contents (unchanged markup)                                   |
|----------|----------------|------------------|---------------------------------------------------------------|
| `repos`  | Repositories   | repo count       | repo list + Add repository                                    |
| `general`| General        | —                | **Global shortcut** (merged in) + Launch at login + Confirm actions |
| `worktrees`| Worktrees    | open-target count| helper text + open-target rows + Add target                   |
| `appearance`| Appearance  | —                | theme cards (renamed from "Theme")                            |
| `graph`  | Graph          | —                | commits-per-page + Show remote branches                       |

Order: Repositories → General → Worktrees → Appearance → Graph.

**Changes vs. today:**
- The standalone `Shortcut` section is merged into `General` (its recorder
  button moves in above the two checkboxes; markup otherwise identical).
- `Theme` section renamed to `Appearance`.
- `Worktrees — open with…` header shortened to `Worktrees`; the explanatory
  `{path}` helper text moves inside the section body (it already lives there).

## Non-goals

- No changes to settings data model, IPC, or any control's behavior.
- No top-tab or sidebar navigation (doesn't fit the narrow window).
- No animation requirement; a plain show/hide is fine (CSS transition optional).

## Testing

Manual, in the running app:
1. Open Settings → only Repositories expanded on first run.
2. Toggle each section header → body shows/hides; chevron flips.
3. Open a couple, close Settings, reopen → same sections still open.
4. Merged: Global shortcut recorder works under General; theme cards work under
   Appearance; badges show correct counts.

## Alternatives considered

- **Category tabs (one section per screen):** cleanest per-screen but 6 labels
  are cramped at 420px and it adds navigation chrome. Rejected for effort/fit.
- **Polished single scroll (bordered cards):** lightest touch but doesn't solve
  navigation — still scroll past everything. Rejected; doesn't meet the goal.
- **Single-open accordion:** forces a close on every switch; feels twitchy in a
  tall window. Rejected in favor of independent toggles.
