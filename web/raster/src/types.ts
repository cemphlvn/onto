// Mirrors onto-runtime::trace (the typed trace projection). The renderer
// reads nothing else: telemetry stays on the Rust side.

export type EventKind =
  | "visit"
  | "claim_wait"
  | "judge_call"
  | "proposer_call"
  | "join_wait"
  | "arrival"
  | "fork"
  | "join"
  | "race_win"
  | "potentiality"
  | "proposal"
  | "admitted"
  | "held"
  | "refused"
  | "escalation"
  | "failed"
  | "position"
  | "surprise"
  | "confirm";

export interface RasterEvent {
  id: number;
  record: string | null;
  category: string;
  frame: string;
  walk: number;
  kind: EventKind;
  start_ns: number;
  end_ns: number | null;
  confidence: number | null;
  mechanical: boolean;
  label: string;
}

export interface Walk {
  id: number;
  parent: number | null;
  root: number;
  case: string | null;
  goal: string;
}

export interface Split { judge_ms: number; proposer_ms: number; claim_wait_ms: number; join_wait_ms: number; other_ms: number }
export interface CallEconomy {
  judge_calls: number; judge_ms: number; proposer_calls: number; proposer_ms: number;
  kinds: { kind: string; escalations: number; proposer_calls: number; proposer_ms: number }[];
  needed_calls: number; needed_ms: number; deferrable_calls: number; deferrable_ms: number;
  unnecessary_calls: number; unnecessary_ms: number; gaps: number; repeated_calls: number;
  avoided_calls: number; fan_out: number; stale_results: number; learned: number; learned_used: number;
  held: number; utility: number; cost_per_resolved_gap_ms: number | null;
}
export interface EnsembleCase {
  job: number; case: string | null; status: string; agreed: string | null; route: string | null;
  first_confirm_ms: number | null; first_surprise_ms: number | null;
  /** (column, object, shared position, ms) */
  positions: [string, string, string, number][];
  last: string | null;
}
export interface Insights {
  ensemble: EnsembleCase[];
  calls: CallEconomy;
  wall_ms: number;
  split: Split;
  concurrency: { max: number; mean: number; profile: [number, number][] };
  frames: { frame: string; visits: number; cases: number; time_ms: number; claim_wait_ms: number; max_queue: number; model_ms: number; escalations: number; escalating_cases: number; proposals: number }[];
  critical_path: { records: string[]; frames: string[]; end_ms: number; split: Split; work_over_wall: number };
  joins: { record: string; frame: string; policy: string; continued: number; arrivals: [number, number][]; decisive: number; spread_ms: number; outcome: string }[];
  learning: { arrow: string; frame: string; at_ms: number; source: string; visits_before: number; escalations_before: number; mean_stay_before_ms: number; visits_after: number; escalations_after: number; mean_stay_after_ms: number; used_after: number }[];
}

export interface Raster {
  insights: Insights;
  category: string;
  frames: string[];
  walks: Walk[];
  events: RasterEvent[];
  parents: Record<string, string[]>;
  meta: {
    category?: string;
    judge?: string;
    proposer?: string;
    policy?: string;
    open_world?: boolean;
    wall_ms?: number;
    model_ms_sum?: number;
    judge_calls?: number;
    proposer_calls?: number;
    learned?: string[];
    ensemble?: string | null;
  };
}

/** What `onto raster` embeds: the projection, and frame records by id. */
export interface Page {
  source: string;
  run: number;
  runs: number;
  raster: Raster;
  records: Record<string, FrameRecord>;
}

export interface FrameRecord {
  id: string;
  at: string;
  primitive: string;
  closure: string;
  after?: string | null;
  tokens?: string[];
  attested?: string[];
  focus?: string | null;
  seen?: unknown;
  outcome?: { kind?: string; arrow?: string; reason?: string; source?: string };
  candidates?: { arrow: string; to: string; judgment: number | null; disposition: unknown; reason: string }[];
  proposals?: { arrow: string; src: string; dst: string }[];
  refused?: [string, string][];
  held?: [string, string][];
  grouped?: { functor: string; chose: string | null; kept: number } | null;
}
