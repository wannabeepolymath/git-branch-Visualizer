// Pure lane-assignment layout for a git commit graph.
// Input: commits in `git log` order (newest first). Output: per-row lane +
// edge segments, virtualization-friendly (each row renders independently).

export interface GraphCommit {
  hash: string;
  parents: string[];
}

export interface GraphRow {
  /** Lane of this commit's node. */
  lane: number;
  /** Lanes entering from the top edge that terminate at this node (children above). */
  ins: number[];
  /** Lanes leaving the node toward the bottom edge (parents below / off-window). */
  outs: number[];
  /** Lanes passing straight through this row without touching the node. */
  passes: number[];
}

export interface GraphLayout {
  rows: GraphRow[];
  maxLanes: number;
}

export const PALETTE = [
  "#3b82f6", // blue
  "#10b981", // emerald
  "#f59e0b", // amber
  "#ef4444", // red
  "#8b5cf6", // violet
  "#ec4899", // pink
  "#06b6d4", // cyan
  "#84cc16", // lime
] as const;

export function laneColor(lane: number): string {
  return PALETTE[lane % PALETTE.length];
}

export function layoutGraph(commits: readonly GraphCommit[]): GraphLayout {
  // lanes[j] = the commit hash this lane is waiting for (its next node), or null if free.
  const lanes: (string | null)[] = [];
  const rows: GraphRow[] = [];
  let maxLanes = 0;

  const firstFree = (): number => {
    const i = lanes.indexOf(null);
    if (i !== -1) return i;
    lanes.push(null);
    return lanes.length - 1;
  };

  for (const c of commits) {
    const activeBefore = lanes.map((h) => h !== null);

    // Every lane waiting for this commit is a child edge coming in from above.
    const ins: number[] = [];
    for (let j = 0; j < lanes.length; j++) if (lanes[j] === c.hash) ins.push(j);

    // Node sits on the leftmost waiting lane; otherwise it's a tip -> new lane.
    const lane = ins.length > 0 ? ins[0] : firstFree();
    for (let k = 1; k < ins.length; k++) lanes[ins[k]] = null; // extra children collapse in

    // First parent continues in the node's lane; extra (merge) parents get their
    // own lane, reusing one that already waits for the same parent if present.
    const outs: number[] = [];
    const firstParent = c.parents.length > 0 ? c.parents[0] : null;
    lanes[lane] = firstParent; // null for a root commit -> lane freed
    if (firstParent !== null) outs.push(lane);
    for (let pi = 1; pi < c.parents.length; pi++) {
      const p = c.parents[pi];
      let pj = lanes.indexOf(p);
      if (pj === -1) {
        pj = firstFree();
        lanes[pj] = p;
      }
      if (!outs.includes(pj)) outs.push(pj);
    }

    // Lanes active before and after this row, untouched by the node, pass through.
    // (A parent lane that already existed both passes through and receives an out
    // curve — that is the correct visual for a merge into an existing lane.)
    const passes: number[] = [];
    for (let j = 0; j < lanes.length; j++) {
      if (j !== lane && activeBefore[j] && lanes[j] !== null && !ins.includes(j)) passes.push(j);
    }

    let width = lane + 1;
    for (const j of ins) width = Math.max(width, j + 1);
    for (const j of outs) width = Math.max(width, j + 1);
    for (const j of passes) width = Math.max(width, j + 1);
    maxLanes = Math.max(maxLanes, width);

    rows.push({ lane, ins, outs, passes });

    while (lanes.length > 0 && lanes[lanes.length - 1] === null) lanes.pop();
  }

  return { rows, maxLanes };
}
