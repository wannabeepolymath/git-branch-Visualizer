// Plain runnable checks for the lane layout. Run: bun src/lib/graph.check.ts
import { layoutGraph, type GraphCommit } from "./graph";

function assert(cond: unknown, msg: string): asserts cond {
  if (!cond) throw new Error(`FAIL: ${msg}`);
}
function eq(actual: unknown, expected: unknown, msg: string): void {
  const a = JSON.stringify(actual);
  const e = JSON.stringify(expected);
  assert(a === e, `${msg} — expected ${e}, got ${a}`);
}
const c = (hash: string, ...parents: string[]): GraphCommit => ({ hash, parents });

// 1. Linear chain stays in lane 0.
{
  const { rows, maxLanes } = layoutGraph([c("c3", "c2"), c("c2", "c1"), c("c1")]);
  assert(rows.every((r) => r.lane === 0), "linear: every commit in lane 0");
  assert(rows.every((r) => r.passes.length === 0), "linear: no pass-through lanes");
  eq(rows[0].ins, [], "linear: tip has no incoming edges");
  eq(rows[2].outs, [], "linear: root has no outgoing edges");
  eq(maxLanes, 1, "linear: one lane total");
}

// 2. Merge commit connects both parent lanes.
{
  const { rows, maxLanes } = layoutGraph([c("m", "a", "b"), c("a", "base"), c("b", "base"), c("base")]);
  eq(rows[0].lane, 0, "merge: merge commit in lane 0");
  eq(rows[0].outs, [0, 1], "merge: edges to both parent lanes");
  eq(rows[1].passes, [1], "merge: second-parent lane passes through row of first parent");
  eq(rows[2].lane, 1, "merge: second parent sits in lane 1");
  eq(rows[3].ins, [0, 1], "merge: base receives both lanes (fork point)");
  eq(rows[3].lane, 0, "merge: base collapses to lane 0");
  eq(maxLanes, 2, "merge: two lanes total");
}

// 3. Fork frees a lane, later tip reuses the freed slot (including a mid-array hole).
{
  // t is an octopus merge (3 parents) -> lanes 0,1,2. x roots -> frees lane 1.
  // New tip w must reuse the freed lane 1, not open lane 3.
  const { rows, maxLanes } = layoutGraph([
    c("t", "a", "x", "y"),
    c("x"),
    c("w", "y"),
    c("y"),
    c("a"),
  ]);
  eq(rows[0].outs, [0, 1, 2], "fork: octopus merge fans out to 3 lanes");
  eq(rows[1].lane, 1, "fork: x sits in lane 1");
  eq(rows[1].outs, [], "fork: x is a root, lane 1 freed");
  eq(rows[2].lane, 1, "fork: new tip w reuses freed lane 1");
  eq(rows[3].ins, [1, 2], "fork: y receives converging edges from lanes 1 and 2");
  eq(rows[3].lane, 1, "fork: y collapses to leftmost waiting lane");
  eq(maxLanes, 3, "fork: never more than 3 lanes");

  // Simple two-tips-one-base fork: second tip opens lane 1, base reclaims it.
  const g2 = layoutGraph([c("d", "b"), c("e", "b"), c("b", "a"), c("f", "a"), c("a")]);
  eq(g2.rows[1].lane, 1, "fork: second tip opens lane 1");
  eq(g2.rows[2].ins, [0, 1], "fork: base joins both children");
  eq(g2.rows[3].lane, 1, "fork: tip f reuses lane 1 freed by b");
}

// 4. Parent outside the loaded window -> dangling edge runs off the bottom.
{
  const { rows, maxLanes } = layoutGraph([c("m", "a", "zzz-not-loaded"), c("a")]);
  eq(rows[0].outs, [0, 1], "dangling: merge edge opens lane 1 for off-window parent");
  eq(rows[1].passes, [1], "dangling: lane 1 still passes through the last loaded row");
  eq(rows[1].outs, [], "dangling: a is a root");
  eq(maxLanes, 2, "dangling: two lanes");
}

console.log("graph.check: all assertions passed");
