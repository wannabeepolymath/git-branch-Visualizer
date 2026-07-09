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

export interface Settings {
  repos: RepoInfo[];
  activeRepoId: string | null;
  shortcut: string;
  launchAtLogin: boolean;
  theme: "system" | "light" | "dark";
  commitsPerPage: number;
  showRemoteBranches: boolean;
}

export const getSettings = (): Promise<Settings> => invoke("get_settings");

export const updateSettings = (settings: Settings): Promise<Settings> =>
  invoke("update_settings", { settings });

export const pickRepoFolder = (): Promise<string | null> => invoke("pick_repo_folder");

export const addRepo = (path: string): Promise<RepoInfo> => invoke("add_repo", { path });

export const removeRepo = (repoId: string): Promise<void> => invoke("remove_repo", { repoId });

export const setActiveRepo = (repoId: string): Promise<void> =>
  invoke("set_active_repo", { repoId });

export const getBranches = (repoId: string): Promise<BranchInfo[]> =>
  invoke("get_branches", { repoId });

export const getLog = (
  repoId: string,
  refs: string[], // empty = all branches/remotes/tags
  skip: number,
  limit: number,
): Promise<CommitInfo[]> => invoke("get_log", { repoId, refs, skip, limit });

export const getCommit = (repoId: string, hash: string): Promise<CommitDetail> =>
  invoke("get_commit", { repoId, hash });

export const getStatus = (repoId: string): Promise<WorkingStatus> =>
  invoke("get_status", { repoId });

export const checkout = (repoId: string, refName: string): Promise<void> =>
  invoke("checkout", { repoId, refName });

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
