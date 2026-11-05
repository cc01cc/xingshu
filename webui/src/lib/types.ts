export type Repo = {
  id: number;
  rootId: number;
  org: string;
  name: string;
  relPath: string;
  remoteUrl: string | null;
  remoteHost: string | null;
  remoteNamespace: string | null;
  orgSource: string | null;
  defaultBranch: string | null;
  headCommit: string | null;
  repoKind: string;
  cloneStatus: string;
  sizeBytes: number;
  lastPullStatus: string | null;
  dirty: boolean;
  ahead: number;
  behind: number;
  statusCheckedAt: string | null;
  lockViolation: boolean;
  modifyLock: boolean;
  tags: string[];
};

export type Tag = { id: number; slug: string; label: string; color: string | null };

export type Root = {
  id: number;
  name: string;
  path: string;
  diskLabel: string | null;
  mountPoint: string | null;
  priority: number;
};

export type Stats = { bytes: number; repositories: number; byKind: Record<string, number> };
