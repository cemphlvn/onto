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
  | "failed";

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

export interface Raster {
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
