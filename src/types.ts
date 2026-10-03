export type AgentState =
  | "idle"
  | "thinking"
  | "executing_tool"
  | "waiting_permission"
  | "verifying"
  | "completed"
  | "failed"
  | "cancelled";

export type ToolStatus = "running" | "ok" | "error" | "denied";

export type ChatEntry =
  | { id: string; kind: "user"; content: string }
  | { id: string; kind: "assistant"; content: string; streaming?: boolean }
  | {
      id: string;
      kind: "tool";
      name: string;
      arguments: unknown;
      risk: string;
      status: ToolStatus;
      output: string;
    }
  | { id: string; kind: "note"; content: string };

export interface PlanStep {
  id: string;
  title: string;
  status: string;
}

export interface FileChange {
  path: string;
  kind: string;
  diff: string;
}

export interface PermissionRequest {
  id: string;
  tool: string;
  arguments: unknown;
  risk: string;
  summary: string;
}

export interface Settings {
  ollamaUrl: string;
  model: string;
  temperature: number;
  contextLength: number;
  maxIterations: number;
  autoApproveSafe: boolean;
  autoApproveFileEdits: boolean;
  terminalTimeoutMs: number;
  systemPrompt: string;
  mode: "agent" | "ask" | "do" | "mission" | "plan";
  workspacePath: string | null;
}

export interface ConversationSummary {
  id: string;
  title: string;
  mode: string;
  updatedAt: number;
  workspacePath: string | null;
}

export interface WorkspaceRecord {
  id: string;
  path: string;
  name: string;
  lastOpenedAt: number;
}

export interface TranscriptItem {
  id: string;
  kind: string;
  content: string;
  name?: string | null;
  arguments?: unknown;
  success?: boolean | null;
  risk?: string | null;
  status?: string | null;
  createdAt: number;
}

export interface ConversationDetail {
  id: string;
  title: string;
  mode: string;
  workspacePath: string | null;
  items: TranscriptItem[];
  plan: PlanStep[];
  fileChanges: FileChange[];
  taskId: string | null;
}

export type AgentEvent =
  | { kind: "state"; state: AgentState; detail?: string | null }
  | { kind: "token"; text: string }
  | { kind: "assistant_done"; content: string }
  | { kind: "tool_started"; id: string; name: string; arguments: unknown; risk: string }
  | { kind: "tool_finished"; id: string; success: boolean; content: string; status: string }
  | { kind: "plan"; steps: PlanStep[] }
  | { kind: "permission"; request: PermissionRequest }
  | { kind: "file_change"; change: FileChange }
  | { kind: "error"; message: string };

export interface ProcessInfo {
  id: string;
  command: string;
  cwd: string;
  running: boolean;
  startedAt: number;
}
