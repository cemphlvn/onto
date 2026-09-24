// The raster view: what an onto run did, in time. Frames on y, time on x,
// one colour family per case, event kinds as shapes, waits and calls as
// lengths. Causality is drawn only for the clicked record (its parents and
// children), so the picture stays readable. ECharts is only the first
// renderer of the typed projection (types.ts).

import * as echarts from "echarts/core";
import { CustomChart, ScatterChart } from "echarts/charts";
import {
  BrushComponent,
  DataZoomComponent,
  GridComponent,
  MarkLineComponent,
  ToolboxComponent,
  TooltipComponent,
} from "echarts/components";
import { CanvasRenderer } from "echarts/renderers";
import type { CustomSeriesOption, ScatterSeriesOption } from "echarts/charts";
import type {
  BrushComponentOption,
  DataZoomComponentOption,
  GridComponentOption,
  ToolboxComponentOption,
  TooltipComponentOption,
} from "echarts/components";
import type { ComposeOption } from "echarts/core";
import type { CustomSeriesRenderItemAPI, CustomSeriesRenderItemParams, CustomSeriesRenderItemReturn } from "echarts";
import type { EventKind, FrameRecord, Page, RasterEvent } from "./types";

echarts.use([
  CustomChart,
  ScatterChart,
  GridComponent,
  TooltipComponent,
  DataZoomComponent,
  BrushComponent,
  ToolboxComponent,
  MarkLineComponent,
  CanvasRenderer,
]);

type Option = ComposeOption<
  | CustomSeriesOption
  | ScatterSeriesOption
  | GridComponentOption
  | TooltipComponentOption
  | DataZoomComponentOption
  | BrushComponentOption
  | ToolboxComponentOption
  | import("echarts/components").MarkLineComponentOption
>;

const page = (window as unknown as { ONTO_RASTER: Page }).ONTO_RASTER;
const R = page.raster;
const $ = (id: string) => document.getElementById(id) as HTMLElement;
const ms = (ns: number) => ns / 1e6;
const fmt = (m: number) => (m >= 1000 ? `${(m / 1000).toFixed(m >= 10000 ? 1 : 2)} s` : `${Math.round(m)} ms`);
const byId = new Map(R.events.map((e) => [e.id, e]));
const walkOf = new Map(R.walks.map((w) => [w.id, w]));
const rowOf = new Map(R.frames.map((f, i) => [f, i]));
const learned = new Set(R.meta.learned ?? []);

const INTERVAL: EventKind[] = ["visit", "claim_wait", "judge_call", "proposer_call", "join_wait"];
const hidden = new Set<number>();

// ---- colour: one hue per case, branches lighter/darker ----
// A case walked by several columns (an ensemble) has one root per column:
// they share the case's hue, as branches do.
const firstRootOfCase = new Map<string, number>();
for (const w of [...R.walks].sort((a, b) => a.id - b.id)) {
  if (w.id === w.root && w.case != null && !firstRootOfCase.has(w.case)) firstRootOfCase.set(w.case, w.id);
}
const rep = (root: number) => {
  const c = walkOf.get(root)?.case;
  return c != null ? firstRootOfCase.get(c) ?? root : root;
};
const roots = [...new Set(R.walks.map((w) => rep(w.root)))].sort((a, b) => a - b);
const hue = new Map(roots.map((r, i) => [r, (i * 137.508 + 200) % 360]));
const family = new Map<number, number[]>();
for (const w of [...R.walks].sort((a, b) => a.id - b.id)) {
  const r = rep(w.root);
  if (!family.has(r)) family.set(r, []);
  family.get(r)!.push(w.id);
}
const dark = () => {
  const t = document.documentElement.getAttribute("data-theme");
  return t ? t === "dark" : window.matchMedia("(prefers-color-scheme: dark)").matches;
};
const css = (v: string) => getComputedStyle(document.documentElement).getPropertyValue(v).trim();
function color(walk: number): string {
  const w = walkOf.get(walk);
  const root = rep(w?.root ?? walk);
  const i = Math.min((family.get(root) ?? [walk]).indexOf(walk), 4);
  const l = dark() ? 66 - i * 6 : 42 + i * 7;
  return `hsl(${hue.get(root) ?? 0} 62% ${l}%)`;
}
const visible = (e: RasterEvent) => !hidden.has(rep(walkOf.get(e.walk)?.root ?? e.walk)) && rowOf.has(e.frame);

// ---- lanes: concurrent visits of one frame each get a sub-row ----
// A crowded frame then shows as a stack of bars (a queue), not one blot.
const MAX_LANES = 10;
const laneOfRecord = new Map<string, number>();
const lanesOfFrame = new Map<string, number>();
{
  const byFrame = new Map<string, RasterEvent[]>();
  for (const e of R.events) if (e.kind === "visit" && e.record) {
    if (!byFrame.has(e.frame)) byFrame.set(e.frame, []);
    byFrame.get(e.frame)!.push(e);
  }
  for (const [frame, vs] of byFrame) {
    vs.sort((a, b) => a.start_ns - b.start_ns);
    const ends: number[] = [];
    for (const v of vs) {
      let lane = ends.findIndex((end) => end <= v.start_ns);
      if (lane < 0) { lane = ends.length < MAX_LANES ? ends.length : ends.indexOf(Math.min(...ends)); if (lane === ends.length) ends.push(0); }
      ends[lane] = v.end_ns ?? v.start_ns;
      laneOfRecord.set(v.record!, lane);
    }
    lanesOfFrame.set(frame, Math.max(1, ends.length));
  }
}
const laneOf = (e: RasterEvent) => (e.record ? laneOfRecord.get(e.record) ?? 0 : 0);
/** Offset of a lane from the row centre, and the lane height, in pixels. */
function lanePx(frame: string, lane: number, band: number): [number, number] {
  const n = lanesOfFrame.get(frame) ?? 1;
  const usable = band * 0.84;
  const h = usable / n;
  return [-usable / 2 + h * (lane + 0.5), h];
}
const bandPx = () => {
  if (!chart) return 40;
  const top = 36, bottom = 64;
  return (chart.getHeight() - top - bottom) / Math.max(1, R.frames.length);
};

// ---- shapes ----
const PATH = {
  fork: "path://M5 0 L5 5 L1 10 M5 5 L9 10",
  join: "path://M1 0 L5 5 L9 0 M5 5 L5 10",
  cross: "path://M0 0 L10 10 M10 0 L0 10",
  bolt: "path://M8 0 L2 8 L7 8 L5 14 L12 5 L7 5 Z",
};
function symbolOf(k: EventKind): { symbol: string; size: number; offset?: [number, number] } {
  switch (k) {
    case "arrival": return { symbol: "circle", size: 8 };
    case "fork": return { symbol: PATH.fork, size: 12 };
    case "join": return { symbol: PATH.join, size: 12 };
    case "race_win": return { symbol: "diamond", size: 13 };
    case "potentiality": return { symbol: "rect", size: 6, offset: [0, 10] };
    case "proposal": return { symbol: "triangle", size: 11, offset: [0, -10] };
    case "admitted": case "held": case "refused": return { symbol: "circle", size: 7, offset: [9, -10] };
    case "escalation": return { symbol: PATH.cross, size: 11 };
    case "failed": return { symbol: "rect", size: 10 };
    case "position": return { symbol: "rect", size: 7 };
    case "surprise": return { symbol: PATH.bolt, size: 16 };
    case "confirm": return { symbol: "circle", size: 13 };
    default: return { symbol: "circle", size: 6 };
  }
}

// ---- series data ----
function intervalData() {
  return R.events
    .filter((e) => INTERVAL.includes(e.kind) && e.end_ns != null && visible(e))
    .map((e) => ({ value: [rowOf.get(e.frame)!, ms(e.start_ns), ms(e.end_ns!), e.id] }));
}
function pointData() {
  return R.events
    .filter((e) => !INTERVAL.includes(e.kind) && visible(e))
    .map((e) => {
      const s = symbolOf(e.kind);
      const [dy] = lanePx(e.frame, laneOf(e), bandPx());
      const c =
        e.kind === "admitted" ? css("--good") :
        e.kind === "held" ? css("--warn") :
        e.kind === "refused" || e.kind === "escalation" || e.kind === "failed" || e.kind === "surprise" ? css("--bad") :
        e.kind === "confirm" ? css("--good") :
        color(e.walk);
      const stroked = e.kind === "fork" || e.kind === "join" || e.kind === "escalation" || e.kind === "confirm";
      const framed = e.kind === "potentiality";
      const opacity = e.kind === "arrival" && !e.mechanical && e.confidence != null ? 0.25 + 0.75 * e.confidence : 1;
      return {
        value: [ms(e.start_ns), rowOf.get(e.frame)!, e.id],
        symbol: s.symbol,
        symbolSize: s.size,
        symbolOffset: [(s.offset ?? [0, 0])[0], (s.offset ?? [0, 0])[1] + dy],
        itemStyle: stroked
          ? { color: "none", borderColor: c, borderWidth: 2, opacity }
          : framed
            ? { color: "transparent", borderColor: c, borderWidth: 1.5, opacity }
            : { color: c, opacity, borderColor: css("--panel"), borderWidth: 1 },
      };
    });
}
// Fork links: short, always shown: a fork to each branch's first arrival.
function forkData() {
  const out: { value: number[] }[] = [];
  for (const f of R.events.filter((e) => e.kind === "fork" && visible(e))) {
    for (const w of R.walks.filter((w) => w.parent === f.walk)) {
      const first = R.events.find((e) => e.walk === w.id && e.kind === "arrival");
      if (first && visible(first) && first.start_ns >= f.start_ns - 1e6) {
        out.push({ value: [ms(f.start_ns), rowOf.get(f.frame)!, ms(first.start_ns), rowOf.get(first.frame)!, w.id, f.id, first.id] });
      }
    }
  }
  return out;
}
// Causal links for the selected record: its parents and its children.
let selected: string | null = null;
function causalData() {
  if (!selected) return [];
  const anchor = (rec: string, last: boolean) => {
    const evs = R.events.filter((e) => e.record === rec && (e.kind === "arrival" || e.kind === "join" || e.kind === "race_win" || e.kind === "fork"));
    return last ? evs[evs.length - 1] : evs[0];
  };
  const pairs: [string, string][] = [];
  for (const p of R.parents[selected] ?? []) pairs.push([p, selected]);
  for (const [child, ps] of Object.entries(R.parents)) if (ps.includes(selected)) pairs.push([selected, child]);
  return pairs
    .map(([a, b]) => [anchor(a, true), anchor(b, false)] as const)
    .filter(([a, b]) => a && b && visible(a) && visible(b))
    .map(([a, b]) => ({ value: [ms(a!.start_ns), rowOf.get(a!.frame)!, ms(b!.start_ns), rowOf.get(b!.frame)!, b!.walk, a!.id, b!.id] }));
}

// ---- render items ----
function renderInterval(params: CustomSeriesRenderItemParams, api: CustomSeriesRenderItemAPI): CustomSeriesRenderItemReturn {
  const e = byId.get(api.value(3) as number)!;
  const row = api.value(0) as number;
  const a = api.coord([api.value(1), row]);
  const b = api.coord([api.value(2), row]);
  const band = (api.size!([0, 1]) as number[])[1];
  const [dy, laneH] = lanePx(e.frame, laneOf(e), band);
  const h =
    e.kind === "visit" ? Math.min(laneH * 0.3, 6) :
    e.kind === "join_wait" ? Math.min(laneH * 0.14, 3) :
    Math.min(laneH * 0.72, 13);
  const cs = params.coordSys as unknown as { x: number; y: number; width: number; height: number };
  const shape = echarts.graphic.clipRectByRect(
    { x: a[0], y: a[1] + dy - h / 2, width: Math.max(1.5, b[0] - a[0]), height: h },
    { x: cs.x, y: cs.y, width: cs.width, height: cs.height },
  );
  if (!shape) return null;
  const c = color(e.walk);
  const style =
    e.kind === "visit" ? { fill: c, opacity: 0.28 } :
    e.kind === "claim_wait" ? { fill: "none", stroke: css("--muted"), lineWidth: 1.2, lineDash: [4, 3] } :
    e.kind === "judge_call" ? { fill: c, opacity: 0.95 } :
    e.kind === "proposer_call" ? { fill: "none", stroke: c, lineWidth: 1.5, lineDash: [3, 2] } :
    { fill: c, opacity: 0.8 };
  return { type: "rect", shape: { ...shape, r: 2 }, style } as CustomSeriesRenderItemReturn;
}
function renderLink(dashed: boolean, width: number, alpha: number) {
  return (_: CustomSeriesRenderItemParams, api: CustomSeriesRenderItemAPI): CustomSeriesRenderItemReturn => {
    const band = (api.size!([0, 1]) as number[])[1];
    const ea = byId.get(api.value(5) as number), eb = byId.get(api.value(6) as number);
    const a = api.coord([api.value(0), api.value(1)]);
    const b = api.coord([api.value(2), api.value(3)]);
    if (ea) a[1] += lanePx(ea.frame, laneOf(ea), band)[0];
    if (eb) b[1] += lanePx(eb.frame, laneOf(eb), band)[0];
    const mx = (a[0] + b[0]) / 2;
    return {
      type: "bezierCurve",
      shape: { x1: a[0], y1: a[1], x2: b[0], y2: b[1], cpx1: mx, cpy1: a[1], cpx2: mx, cpy2: b[1] },
      style: { stroke: color(api.value(4) as number), lineWidth: width, opacity: alpha, fill: "none", lineDash: dashed ? [3, 3] : undefined },
      silent: true,
    } as CustomSeriesRenderItemReturn;
  };
}

// ---- chart ----
let chart: echarts.ECharts | null = null;
function option(): Option {
  const tMax = Math.max(1, ...R.events.map((e) => ms(e.end_ns ?? e.start_ns)));
  return {
    backgroundColor: "transparent",
    animation: false,
    textStyle: { fontFamily: "ui-sans-serif, system-ui, -apple-system, Segoe UI, sans-serif" },
    grid: { left: 150, right: 24, top: 36, bottom: 64 },
    toolbox: { right: 16, top: 0, feature: { brush: { type: ["lineX", "clear"], title: { lineX: "select a time window", clear: "clear" } } } },
    brush: { xAxisIndex: 0, brushType: "lineX", throttleType: "debounce", throttleDelay: 200, brushStyle: { color: "rgba(120,120,120,0.15)" } },
    tooltip: {
      trigger: "item",
      confine: true,
      formatter: (p: unknown) => {
        const d = (p as { value?: number[] }).value;
        const id = d ? d[d.length === 3 ? 2 : 3] : undefined;
        const e = id != null ? byId.get(id) : undefined;
        if (!e) return "";
        const w = walkOf.get(e.walk);
        const who = `${w?.case ? w.case + " · " : ""}walk ${e.walk}`;
        return `<b>${esc(e.label)}</b><br><span style="opacity:.7">${esc(who)} · ${e.kind.replace("_", " ")} · ${fmt(ms(e.start_ns))}</span>`;
      },
    },
    xAxis: {
      type: "value",
      min: 0,
      max: tMax,
      axisLabel: { formatter: (v: number) => fmt(v), color: css("--muted") },
      splitLine: { lineStyle: { color: css("--grid") } },
    },
    yAxis: {
      type: "category",
      data: R.frames,
      inverse: true,
      axisTick: { show: false },
      axisLine: { lineStyle: { color: css("--border") } },
      axisLabel: {
        color: css("--fg"),
        formatter: (v: string) => (learned.has(v) ? `{l|${v}}` : v),
        rich: { l: { fontStyle: "italic", color: css("--muted") } },
      },
      splitArea: { show: true, areaStyle: { color: ["transparent", css("--band")] } },
    },
    dataZoom: [
      { type: "inside", xAxisIndex: 0, filterMode: "weakFilter" },
      { type: "slider", xAxisIndex: 0, filterMode: "weakFilter", height: 18, bottom: 16, labelFormatter: (v: number) => fmt(v) },
      ...(R.frames.length > 14 ? [{ type: "slider" as const, yAxisIndex: 0, filterMode: "weakFilter" as const, width: 14, right: 4 }] : []),
    ],
    series: [
      { id: "intervals", type: "custom", renderItem: renderInterval, encode: { x: [1, 2], y: 0 }, data: intervalData(), clip: true },
      { id: "forks", type: "custom", renderItem: renderLink(false, 1.2, 0.6), encode: { x: [0, 2], y: [1, 3] }, data: forkData(), silent: true },
      { id: "causal", type: "custom", renderItem: renderLink(true, 2, 0.9), encode: { x: [0, 2], y: [1, 3] }, data: causalData(), silent: true, z: 5 },
      {
        id: "points", type: "scatter", data: pointData(), encode: { x: 0, y: 1 }, z: 10, emphasis: { scale: 1.4 },
        // Learning boundaries: from here on, the learned arrow is active.
        markLine: {
          silent: true, symbol: "none",
          lineStyle: { color: css("--good"), type: "dashed", width: 1.2 },
          label: { color: css("--good"), formatter: (p: unknown) => (p as { name?: string }).name ?? "", position: "insideEndTop", fontSize: 11 },
          data: R.insights.learning.map((l) => ({ name: `learned ${l.arrow}`, xAxis: l.at_ms })),
        },
      },
    ],
  };
}
function mount() {
  chart?.dispose();
  chart = echarts.init($("chart"), dark() ? "dark" : undefined, { renderer: "canvas" });
  chart.setOption(option());
  refresh(); // lane offsets of points need the chart's size
  chart.on("click", (p) => {
    const d = (p as { value?: number[] }).value;
    if (!d) return;
    const id = d[d.length === 3 ? 2 : 3];
    const e = byId.get(id);
    if (e) inspect(e);
  });
  chart.on("brushSelected", (p) => {
    const areas = (p as { batch?: { areas?: { coordRange?: number[] }[] }[] }).batch?.[0]?.areas ?? [];
    const range = areas[0]?.coordRange;
    if (range) windowSummary(range[0], range[1]);
  });
}
function refresh() {
  chart?.setOption({
    series: [
      { id: "intervals", data: intervalData() },
      { id: "forks", data: forkData() },
      { id: "causal", data: causalData() },
      { id: "points", data: pointData() },
    ],
  });
}

// ---- header ----
function header() {
  const m = R.meta;
  $("sub").textContent = [m.category, m.judge && `judge ${m.judge}`, m.proposer && `proposer ${m.proposer}`, m.policy && `policy ${m.policy}`, page.source, page.runs > 1 ? `run ${page.run} of ${page.runs}` : ""].filter(Boolean).join(" · ");
  const stats: [string | number, string][] = [
    [roots.length, "cases"],
    [R.walks.length, "walks"],
    [fmt(m.wall_ms ?? Math.max(...R.events.map((e) => ms(e.end_ns ?? e.start_ns)))), "wall"],
  ];
  if (m.model_ms_sum && m.wall_ms) stats.push([`${(m.model_ms_sum / m.wall_ms).toFixed(2)}×`, "parallelism"]);
  stats.push([m.judge_calls ?? count("judge_call"), "judge calls"], [m.proposer_calls ?? count("proposer_call"), "proposer calls"]);
  stats.push([count("escalation"), "escalations"], [fmt(total("claim_wait")), "claim waits"], [fmt(total("join_wait")), "join waits"]);
  $("stats").replaceChildren(...stats.map(([v, k]) => {
    const d = document.createElement("div"); d.className = "stat";
    const b = document.createElement("b"); b.textContent = String(v);
    const s = document.createElement("span"); s.textContent = k;
    d.append(b, s); return d;
  }));
  $("cases").replaceChildren(...roots.map((r) => {
    const w = walkOf.get(r)!, b = document.createElement("button");
    b.className = "chip"; b.setAttribute("aria-pressed", hidden.has(r) ? "false" : "true");
    const dot = document.createElement("span"); dot.className = "dot"; dot.style.background = color(r);
    const goal = w.goal.length > 40 ? w.goal.slice(0, 39) + "…" : w.goal;
    b.append(dot, document.createTextNode(`${w.case ?? "walk " + r} · ${goal}`)); b.title = w.goal;
    b.onclick = () => { hidden.has(r) ? hidden.delete(r) : hidden.add(r); b.setAttribute("aria-pressed", hidden.has(r) ? "false" : "true"); refresh(); };
    return b;
  }));
}
const count = (k: EventKind) => R.events.filter((e) => e.kind === k).length;
const total = (k: EventKind) => R.events.filter((e) => e.kind === k).reduce((s, e) => s + ms((e.end_ns ?? e.start_ns) - e.start_ns), 0);

// ---- inspector ----
function esc(s: string) { return s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c]!)); }
function panel(title: string, rows: [string, string][]): HTMLElement {
  const box = document.createElement("section");
  const h = document.createElement("h2"); h.textContent = title; box.appendChild(h);
  const dl = document.createElement("dl");
  for (const [k, v] of rows) {
    if (!v) continue;
    const dt = document.createElement("dt"); dt.textContent = k;
    const dd = document.createElement("dd"); dd.textContent = v;
    dl.append(dt, dd);
  }
  box.appendChild(dl);
  return box;
}
function inspect(e: RasterEvent) {
  selected = e.record;
  refresh();
  const w = walkOf.get(e.walk);
  const out: HTMLElement[] = [panel(e.label, [
    ["kind", e.kind.replace("_", " ")],
    ["case", `${w?.case ?? ""} walk ${e.walk}${w?.parent != null ? ` (branch of walk ${w.parent})` : ""}`],
    ["at", e.frame],
    ["time", e.end_ns != null ? `${fmt(ms(e.start_ns))} → ${fmt(ms(e.end_ns))} (${fmt(ms(e.end_ns - e.start_ns))})` : fmt(ms(e.start_ns))],
    ["confidence", e.mechanical ? "mechanical step" : e.confidence != null ? e.confidence.toFixed(2) : ""],
    ["record", e.record ?? ""],
    ["caused by", (e.record && R.parents[e.record]?.join(", ")) || ""],
    ["leads to", e.record ? Object.entries(R.parents).filter(([, ps]) => ps.includes(e.record!)).map(([c]) => c).join(", ") : ""],
  ])];
  const rec: FrameRecord | undefined = e.record ? page.records[e.record] : undefined;
  if (rec) out.push(record(rec));
  else {
    const p = document.createElement("p"); p.className = "empty";
    p.textContent = Object.keys(page.records).length ? "No frame record for this event." : "Pass --dispositions to onto raster to see frame records here.";
    out.push(p);
  }
  $("details").replaceChildren(...out);
}
function record(rec: FrameRecord): HTMLElement {
  const o = rec.outcome ?? {};
  const box = panel(`Frame record ${rec.id}`, [
    ["at", `${rec.at} (${rec.primitive}, ${rec.closure})`],
    ["tokens", (rec.tokens ?? []).join(", ")],
    ["attested", (rec.attested ?? []).join("; ")],
    ["focus", rec.focus ?? ""],
    ["model saw", rec.seen ? leaves(rec.seen).join(", ") : ""],
    ["grouped", rec.grouped ? `${rec.grouped.functor}: ${rec.grouped.chose ?? "fallback"} (kept ${rec.grouped.kept})` : ""],
    ["outcome", [o.kind, o.arrow && `→ ${o.arrow}`, o.reason, o.source].filter(Boolean).join(" ")],
  ]);
  for (const c of rec.candidates ?? []) {
    const d = document.createElement("div"); d.className = "cand";
    const disp = typeof c.disposition === "string" ? c.disposition : (c.disposition as { kind?: string })?.kind ?? "";
    const top = document.createElement("div");
    top.innerHTML = `<span class="k"></span> <span class="tag"></span> <span class="tag"></span>`;
    (top.children[0] as HTMLElement).textContent = `${c.arrow} → ${c.to}`;
    (top.children[1] as HTMLElement).textContent = disp;
    (top.children[2] as HTMLElement).textContent = c.judgment != null ? c.judgment.toFixed(2) : "—";
    const r = document.createElement("div"); r.className = "r"; r.textContent = c.reason;
    d.append(top, r); box.appendChild(d);
  }
  for (const [label, list] of [
    ["proposed", (rec.proposals ?? []).map((p) => `${p.arrow}: ${p.src} → ${p.dst}`)],
    ["not learned", (rec.refused ?? []).map((x) => x.join(": "))],
    ["held for a person", (rec.held ?? []).map((x) => x.join(": "))],
  ] as [string, string[]][]) {
    if (!list.length) continue;
    const d = document.createElement("div"); d.className = "cand";
    const k = document.createElement("div"); k.className = "k"; k.textContent = label; d.appendChild(k);
    for (const s of list) { const r = document.createElement("div"); r.className = "r"; r.textContent = s; d.appendChild(r); }
    box.appendChild(d);
  }
  return box;
}
function leaves(v: unknown, prefix = ""): string[] {
  if (v && typeof v === "object" && !Array.isArray(v) && Object.keys(v).length) {
    return Object.entries(v as Record<string, unknown>).flatMap(([k, x]) =>
      !prefix && (k === "at" || k === "about_at") ? [] : leaves(x, prefix ? `${prefix}.${k}` : k));
  }
  return [prefix];
}

// ---- brush: a time window ----
function windowSummary(a: number, b: number) {
  const inWin = R.events.filter((e) => visible(e) && ms(e.start_ns) <= b && ms(e.end_ns ?? e.start_ns) >= a);
  const overlap = (e: RasterEvent) => Math.max(0, Math.min(b, ms(e.end_ns ?? e.start_ns)) - Math.max(a, ms(e.start_ns)));
  const sum = (k: EventKind) => inWin.filter((e) => e.kind === k).reduce((s, e) => s + overlap(e), 0);
  const n = (k: EventKind) => inWin.filter((e) => e.kind === k).length;
  const walks = new Set(inWin.filter((e) => e.kind === "visit").map((e) => e.walk));
  const frames = new Map<string, number>();
  for (const e of inWin.filter((e) => e.kind === "visit")) frames.set(e.frame, (frames.get(e.frame) ?? 0) + overlap(e));
  const busiest = [...frames.entries()].sort((x, y) => y[1] - x[1]).slice(0, 3).map(([f, t]) => `${f} ${fmt(t)}`).join(", ");
  selected = null; refresh();
  $("details").replaceChildren(panel(`Window ${fmt(a)} → ${fmt(b)}`, [
    ["walks active", String(walks.size)],
    ["judge time", `${fmt(sum("judge_call"))} (${n("judge_call")} calls)`],
    ["proposer time", `${fmt(sum("proposer_call"))} (${n("proposer_call")} calls)`],
    ["claim waits", fmt(sum("claim_wait"))],
    ["join waits", fmt(sum("join_wait"))],
    ["arrivals", String(n("arrival"))],
    ["escalations", String(n("escalation"))],
    ["learned / held / refused", `${n("admitted")} / ${n("held")} / ${n("refused")}`],
    ["busiest frames", busiest],
  ]));
}

// ---- legend ----
function legend() {
  const items: [string, string][] = [
    ["visit", `<svg width="22" height="12"><rect x="1" y="4" width="20" height="4" rx="2" fill="currentColor" opacity=".35"/></svg>`],
    ["claim wait", `<svg width="22" height="12"><rect x="1.5" y="2.5" width="19" height="7" fill="none" stroke="currentColor" stroke-dasharray="4 3"/></svg>`],
    ["judge call", `<svg width="22" height="12"><rect x="1" y="2" width="20" height="8" rx="2" fill="currentColor"/></svg>`],
    ["proposer call", `<svg width="22" height="12"><rect x="1.5" y="2.5" width="19" height="7" rx="2" fill="none" stroke="currentColor" stroke-dasharray="3 2"/></svg>`],
    ["join wait", `<svg width="22" height="12"><rect x="1" y="5" width="20" height="2" fill="currentColor"/></svg>`],
    ["arrival (opacity: confidence)", `<svg width="14" height="12"><circle cx="7" cy="6" r="4" fill="currentColor"/></svg>`],
    ["fork", `<svg width="14" height="12"><path d="M7 1 L7 6 L3 11 M7 6 L11 11" stroke="currentColor" fill="none" stroke-width="1.8"/></svg>`],
    ["join", `<svg width="14" height="12"><path d="M3 1 L7 6 L11 1 M7 6 L7 11" stroke="currentColor" fill="none" stroke-width="1.8"/></svg>`],
    ["race winner", `<svg width="14" height="12"><path d="M7 0 L13 6 L7 12 L1 6 Z" fill="currentColor"/></svg>`],
    ["potentiality", `<svg width="14" height="12"><rect x="3" y="2" width="8" height="8" fill="none" stroke="currentColor"/></svg>`],
    ["proposal / transport", `<svg width="14" height="12"><path d="M7 1 L13 11 L1 11 Z" fill="currentColor"/></svg>`],
    ["learned", `<svg width="12" height="12"><circle cx="6" cy="6" r="4" fill="var(--good)"/></svg>`],
    ["held for a person", `<svg width="12" height="12"><circle cx="6" cy="6" r="4" fill="var(--warn)"/></svg>`],
    ["refused", `<svg width="12" height="12"><circle cx="6" cy="6" r="4" fill="var(--bad)"/></svg>`],
    ["escalation", `<svg width="14" height="12"><path d="M2 1 L12 11 M12 1 L2 11" stroke="var(--bad)" stroke-width="2"/></svg>`],
  ];
  if (R.insights.ensemble.length) items.push(
    ["a column's position (shared band)", `<svg width="12" height="12"><rect x="2" y="2" width="8" height="8" fill="currentColor"/></svg>`],
    ["confirmed by every column", `<svg width="14" height="14"><circle cx="7" cy="7" r="5" fill="none" stroke="var(--good)" stroke-width="2"/></svg>`],
    ["surprise: perspectives disagree", `<svg width="14" height="14"><path d="M8 0 L2 8 L7 8 L5 14 L12 5 L7 5 Z" fill="var(--bad)"/></svg>`],
  );
  $("legend").innerHTML = items.map(([k, s]) => `<span>${s}${esc(k)}</span>`).join("");
}

// ---- insights: what the raster shows, measured ----
function insights() {
  const I = R.insights;
  selected = null; refresh();
  const out: HTMLElement[] = [];
  const bar = (s: import("./types").Split) => {
    const parts: [string, number, string][] = [
      ["judge", s.judge_ms, "var(--fg)"], ["proposer", s.proposer_ms, "var(--muted)"],
      ["claim wait", s.claim_wait_ms, "var(--warn)"], ["join wait", s.join_wait_ms, "var(--bad)"], ["other", s.other_ms, "var(--grid)"],
    ];
    const tot = parts.reduce((a, p) => a + p[1], 0) || 1;
    const d = document.createElement("div"); d.className = "split";
    d.innerHTML = `<div class="split-bar">${parts.map(([, v, c]) => `<i style="width:${(100 * v) / tot}%;background:${c}"></i>`).join("")}</div>` +
      `<div class="split-keys">${parts.filter(([, v]) => v > 0.5).map(([k, v, c]) => `<span><i style="background:${c}"></i>${esc(k)} ${fmt(v)}</span>`).join("")}</div>`;
    return d;
  };
  const sec = (title: string, ...kids: (HTMLElement | string)[]) => {
    const s = document.createElement("section");
    const h = document.createElement("h2"); h.textContent = title; s.appendChild(h);
    for (const k of kids) { if (typeof k === "string") { const p = document.createElement("p"); p.className = "r"; p.textContent = k; s.appendChild(p); } else s.appendChild(k); }
    return s;
  };
  const who = (w: number) => { const x = walkOf.get(w); return `${x?.case ?? ""} walk ${w}`.trim(); };
  const list = (rows: string[]) => { const ul = document.createElement("ul"); ul.className = "ins"; for (const r of rows) { const li = document.createElement("li"); li.textContent = r; ul.appendChild(li); } return ul; };

  if (I.ensemble.length) {
    const said = (c: import("./types").EnsembleCase) => c.positions.map(([col, obj, sh]) => `${col} ${obj} → ${sh}`).join(" · ");
    out.push(sec(`Perspectives (${R.meta.ensemble ?? "ensemble"})`, list(I.ensemble.map((c) => {
      const name = c.case ?? `case ${c.job + 1}`;
      const last = c.last ? `; last to conclude: ${c.last}` : "";
      switch (c.status) {
        case "agreed": return `${name} agreed on ${c.agreed}${c.first_confirm_ms != null ? ` at ${fmt(c.first_confirm_ms)}` : ""}: ${said(c)}${last}`;
        case "surprise": return `${name} surprise at ${fmt(c.first_surprise_ms ?? 0)} → ${c.route === "person" ? "a person" : "curation"}: ${said(c)}${last}`;
        default: return `${name} ${c.status}: ${said(c)}`;
      }
    })), "Agreement is structural: two positions agree when one reaches the other in the shared category."));
  }
  out.push(sec("Where the time went (all walks)", bar(I.split),
    `Model ${fmt(I.split.judge_ms + I.split.proposer_ms)} · coordination ${fmt(I.split.claim_wait_ms + I.split.join_wait_ms)} over a ${fmt(I.wall_ms)} run.`));
  out.push(sec("Parallelism",
    `Up to ${I.concurrency.max} walks active at once; ${I.concurrency.mean.toFixed(2)} on average. Work over wall: ${I.critical_path.work_over_wall.toFixed(2)}×.`));
  const C = I.calls;
  if (C.proposer_calls) {
    const parts: [string, number, string][] = [
      ["needed now", C.needed_ms, "var(--good)"], ["deferrable (review only)", C.deferrable_ms, "var(--warn)"], ["unnecessary", C.unnecessary_ms, "var(--bad)"],
    ];
    const tot = parts.reduce((a, p) => a + p[1], 0) || 1;
    const d = document.createElement("div"); d.className = "split";
    d.innerHTML = `<div class="split-bar">${parts.map(([, v, c]) => `<i style="width:${(100 * v) / tot}%;background:${c}"></i>`).join("")}</div>` +
      `<div class="split-keys">${parts.filter(([, v]) => v > 0.5).map(([k, v, c]) => `<span><i style="background:${c}"></i>${esc(k)} ${fmt(v)}</span>`).join("")}</div>`;
    out.push(sec("Proposer calls: what they were for", d,
      `${C.proposer_calls} calls for ${C.gaps} gaps (${C.repeated_calls} repeats, ${C.avoided_calls} avoided by sharing, fan-out ${C.fan_out.toFixed(2)}). ` +
      `${C.learned} learned, ${C.learned_used} used, ${C.held} held: utility ${(100 * C.utility).toFixed(0)}%` +
      (C.cost_per_resolved_gap_ms != null ? `, ${fmt(C.cost_per_resolved_gap_ms)} of proposer time per resolved gap.` : "."),
      list(C.kinds.map((k) => `${k.kind.replace("_", " ")}: ${k.escalations} escalations, ${k.proposer_calls} proposer calls, ${fmt(k.proposer_ms)}`))));
  }
  const cp = I.critical_path;
  out.push(sec("Critical path (what the run waited for)", bar(cp.split),
    `${cp.frames.join(" → ")} · ends at ${fmt(cp.end_ms)} (${cp.records.length} records)`));
  const busy = I.frames.filter((f) => f.visits > 0).slice(0, 5);
  out.push(sec("Busiest frames", list(busy.map((f) =>
    `${f.frame}: ${fmt(f.time_ms)} in ${f.visits} visits (${f.cases} cases) · model ${fmt(f.model_ms)} · claim wait ${fmt(f.claim_wait_ms)}${f.max_queue > 1 ? `, queue up to ${f.max_queue}` : ""}`))));
  if (I.joins.length) out.push(sec("Joins", list(I.joins.map((j) =>
    j.outcome === "incomplete"
      ? `${j.frame} (${j.policy}) incomplete: ${who(j.decisive)} never arrived; ${j.arrivals.length} branch(es) waited ${fmt(j.spread_ms)}`
      : j.policy === "race"
      ? `${j.frame} (race): ${who(j.decisive)} won${j.arrivals.length > 1 ? `, the next ${fmt(j.spread_ms)} later` : ""}`
      : `${j.frame} (${j.policy}): waited for ${who(j.decisive)}, the last of ${j.arrivals.length}, ${fmt(j.spread_ms)} after the first`))));
  const gaps = I.frames.filter((f) => f.escalations > 1).sort((a, b) => b.escalations - a.escalations);
  if (gaps.length) out.push(sec("Repeated gaps", list(gaps.map((f) =>
    `${f.frame}: ${f.escalations} escalations from ${f.escalating_cases} cases, ${f.proposals} proposals`))));
  if (I.learning.length) out.push(sec("Learning, before → after", list(I.learning.map((l) =>
    `${l.arrow} at ${l.frame} (${l.source}, ${fmt(l.at_ms)}): escalations ${l.escalations_before} → ${l.escalations_after}, mean stay ${fmt(l.mean_stay_before_ms)} → ${fmt(l.mean_stay_after_ms)}, used ${l.used_after}× after`))));
  $("details").replaceChildren(...out);
}

function all() { header(); legend(); mount(); insights(); }
$("show-insights")?.addEventListener("click", insights);
// A small hook for tests and scripted exploration (no behaviour of its own).
(window as unknown as { __ontoRaster: object }).__ontoRaster = {
  events: R.events,
  inspect: (id: number) => { const e = byId.get(id); if (e) inspect(e); },
  window: windowSummary,
};
all();
window.addEventListener("resize", () => { chart?.resize(); refresh(); });
window.matchMedia("(prefers-color-scheme: dark)").addEventListener?.("change", all);
new MutationObserver(all).observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
