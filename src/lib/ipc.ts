// Typed wrappers around the Tauri IPC contract. All commands may reject with a
// string error — callers surface it via the toast.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface RepoInfo {
  id: string;
  name: string;
  path: string;
}

export interface BranchInfo {
  name: string;
  isCurrent: boolean;
  isRemote: boolean;
  upstream: string | null;
  ahead: number;
  behind: number;
  lastCommitTime: number; // unix secs
  lastCommitSubject: string;
}

export interface CommitInfo {
  hash: string;
  parents: string[];
  subject: string;
  authorName: string;
  authorEmail: string;
  timestamp: number;
  refs: string[]; // "main", "origin/main", "tag:v1.0"
}

export interface FileChange {
  status: string;
  path: string;
}

export type CommitDetail = CommitInfo & {
  body: string;
  files: FileChange[];
};

export interface WorkingStatus {
  staged: FileChange[];
  unstaged: FileChange[]; // untracked files appear here with status "?"
}

export interface WorktreeInfo {
  path: string;
  branch: string | null; // null = detached HEAD (or bare main)
  head: string; // full SHA; shorten for detached display
  isMain: boolean;
  dirty: boolean;
  locked: boolean;
  prunable: boolean;
}

/** Visual identity. Each is a complete, self-contained look (see index.css). */
export type ThemeName = "graphite" | "paper" | "terminal";
export const THEME_NAMES: ThemeName[] = ["graphite", "paper", "terminal"];

/** Map any stored value (incl. legacy "system"/"light"/"dark") to a real theme. */
export function normalizeTheme(t: string): ThemeName {
  return t === "paper" || t === "terminal" ? t : "graphite";
}

export interface Settings {
  repos: RepoInfo[];
  activeRepoId: string | null;
  shortcut: string;
  launchAtLogin: boolean;
  theme: string; // one of ThemeName; normalizeTheme() guards legacy values
  commitsPerPage: number;
  showRemoteBranches: boolean;
  confirmActions: boolean;
}

export const getSettings = (): Promise<Settings> => invoke("get_settings");

/** Re-anchor the popover window under the tray icon. */
export const recenterWindow = (): Promise<void> => invoke("recenter_window");

export const updateSettings = (settings: Settings): Promise<Settings> =>
  invoke("update_settings", { settings });

export const pickRepoFolder = (): Promise<string | null> => invoke("pick_repo_folder");

export const addRepo = (path: string): Promise<RepoInfo> => invoke("add_repo", { path });

export const removeRepo = (repoId: string): Promise<void> => invoke("remove_repo", { repoId });

export const setActiveRepo = (repoId: string): Promise<void> =>
  invoke("set_active_repo", { repoId });

export const getBranches = (repoId: string): Promise<BranchInfo[]> =>
  invoke("get_branches", { repoId });

export const getWorktrees = (repoId: string): Promise<WorktreeInfo[]> =>
  invoke("get_worktrees", { repoId });

export const getLog = (
  repoId: string,
  refs: string[], // empty = all branches/remotes/tags
  skip: number,
  limit: number,
): Promise<CommitInfo[]> => invoke("get_log", { repoId, refs, skip, limit });

export const getCommit = (repoId: string, hash: string): Promise<CommitDetail> =>
  invoke("get_commit", { repoId, hash });

// `worktreePath` routes the action to a linked worktree's working dir. Omit (or
// pass undefined) to act on the repo's main worktree.
export const getStatus = (repoId: string, worktreePath?: string): Promise<WorkingStatus> =>
  invoke("get_status", { repoId, worktreePath });

/** Unified diff for one path. `staged` = index vs HEAD; else worktree vs index. */
export const diffFile = (
  repoId: string,
  path: string,
  staged: boolean,
  untracked: boolean,
  worktreePath?: string,
): Promise<string> => invoke("diff_file", { repoId, path, staged, untracked, worktreePath });

export const stageFiles = (repoId: string, paths: string[], worktreePath?: string): Promise<void> =>
  invoke("stage_files", { repoId, paths, worktreePath });

export const unstageFiles = (
  repoId: string,
  paths: string[],
  worktreePath?: string,
): Promise<void> => invoke("unstage_files", { repoId, paths, worktreePath });

export const discardFiles = (
  repoId: string,
  paths: string[],
  untracked: boolean,
  worktreePath?: string,
): Promise<void> => invoke("discard_files", { repoId, paths, untracked, worktreePath });

export const checkout = (repoId: string, refName: string, worktreePath?: string): Promise<void> =>
  invoke("checkout", { repoId, refName, worktreePath });

export const createBranch = (repoId: string, name: string, fromRef: string): Promise<void> =>
  invoke("create_branch", { repoId, name, fromRef });

export const renameBranch = (repoId: string, oldName: string, newName: string): Promise<void> =>
  invoke("rename_branch", { repoId, oldName, newName });

export const deleteBranch = (repoId: string, name: string): Promise<void> =>
  invoke("delete_branch", { repoId, name });

export const fetchRepo = (repoId: string): Promise<void> => invoke("fetch_repo", { repoId });

export const pullRepo = (repoId: string): Promise<void> => invoke("pull_repo", { repoId });

/** Subscribe to backend repo-change notifications. Resolves to an unlisten fn. */
export function onRepoChanged(cb: (repoId: string) => void): Promise<UnlistenFn> {
  return listen<{ repoId: string }>("repo-changed", (e) => cb(e.payload.repoId));
}
