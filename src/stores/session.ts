import { create } from "zustand";

import type { RunMode } from "../lib/catalog";
import { translate, wasDenied } from "../lib/i18n";
import { useUi } from "./ui";
import type {
  AgentEvent,
  AgentState,
  ChatEntry,
  ConversationSummary,
  FileChange,
  PermissionRequest,
  PlanStep,
  ProcessInfo,
  ToolStatus,
  TranscriptItem,
  WorkspaceRecord,
} from "../types";

export interface Proposal {
  phase: "drafting" | "ready";
  mode: Exclude<RunMode, "ask">;
  goal: string;
  text: string;
  editing: boolean;
}

interface SessionStore {
  view: "chat" | "settings" | "skills" | "automations" | "plugins" | "archive";
  workspacePath: string | null;
  workspaces: WorkspaceRecord[];
  conversations: ConversationSummary[];
  conversationId: string | null;
  taskId: string | null;
  messages: ChatEntry[];
  agentState: AgentState;
  statusDetail: string | null;
  plan: PlanStep[];
  fileChanges: FileChange[];
  permission: PermissionRequest | null;
  processes: ProcessInfo[];
  selectedDiff: string | null;
  inspectorOpen: boolean;
  inspectorTab: "plan" | "files" | "sources" | "terminal";
  ollamaOnline: boolean;
  ollamaMessage: string;
  models: string[];
  banner: string | null;
  computerHome: string | null;
  desktopPath: string | null;
  attachments: string[];
  proposal: Proposal | null;
  automationWatch: { id: string; background: boolean } | null;
  setView: (view: "chat" | "settings" | "skills" | "automations" | "plugins" | "archive") => void;
  setComputerHome: (path: string) => void;
  setDesktopPath: (path: string) => void;
  setAttachments: (paths: string[]) => void;
  setProposal: (proposal: Proposal | null) => void;
  setAutomationWatch: (watch: { id: string; background: boolean } | null) => void;
  setWorkspace: (path: string | null) => void;
  setWorkspaces: (workspaces: WorkspaceRecord[]) => void;
  setConversations: (conversations: ConversationSummary[]) => void;
  setOllama: (online: boolean, message: string, models: string[]) => void;
  setBanner: (banner: string | null) => void;
  setProcesses: (processes: ProcessInfo[]) => void;
  openConversation: (detail: {
    id: string;
    items: TranscriptItem[];
    plan: PlanStep[];
    fileChanges: FileChange[];
    taskId: string | null;
    workspacePath: string | null;
  }) => void;
  newTask: (workspacePath?: string | null) => void;
  pushUser: (content: string) => void;
  beginRun: (conversationId: string, taskId: string) => void;
  applyEvent: (event: AgentEvent) => void;
  setInspector: (tab: SessionStore["inspectorTab"]) => void;
  toggleInspector: () => void;
  selectDiff: (path: string | null) => void;
}

function transcriptToEntries(items: TranscriptItem[]): ChatEntry[] {
  return [...items]
    .sort((a, b) => a.createdAt - b.createdAt)
    .map((item) => {
      if (item.kind === "tool") {
        const denied = wasDenied(item.content);
        const status: ToolStatus = denied ? "denied" : item.success ? "ok" : item.status === "running" ? "running" : "error";
        return {
          id: item.id,
          kind: "tool" as const,
          name: item.name ?? "tool",
          arguments: item.arguments ?? {},
          risk: item.risk ?? "safe",
          status,
          output: item.content,
        };
      }
      if (item.kind === "user") {
        return { id: item.id, kind: "user" as const, content: item.content };
      }
      return { id: item.id, kind: "assistant" as const, content: item.content };
    });
}

export const useSession = create<SessionStore>((set, get) => ({
  view: "chat",
  workspacePath: null,
  workspaces: [],
  conversations: [],
  conversationId: null,
  taskId: null,
  messages: [],
  agentState: "idle",
  statusDetail: null,
  plan: [],
  fileChanges: [],
  permission: null,
  processes: [],
  selectedDiff: null,
  inspectorOpen: true,
  inspectorTab: "files",
  ollamaOnline: false,
  ollamaMessage: "Checking Ollama.",
  models: [],
  banner: null,
  computerHome: null,
  desktopPath: null,
  attachments: [],
  proposal: null,
  automationWatch: null,
  setView: (view) => set({ view }),
  setComputerHome: (computerHome) => set({ computerHome }),
  setDesktopPath: (desktopPath) => set({ desktopPath }),
  setAttachments: (attachments) => set({ attachments }),
  setProposal: (proposal) => set({ proposal }),
  setAutomationWatch: (automationWatch) => set({ automationWatch }),
  setWorkspace: (workspacePath) => set({ workspacePath }),
  setWorkspaces: (workspaces) => set({ workspaces }),
  setConversations: (conversations) => set({ conversations }),
  setOllama: (ollamaOnline, ollamaMessage, models) => set({ ollamaOnline, ollamaMessage, models }),
  setBanner: (banner) => set({ banner: banner?.includes("invoke") ? null : banner }),
  setProcesses: (processes) => set({ processes }),
  openConversation: (detail) =>
    set({
      view: "chat",
      conversationId: detail.id,
      taskId: detail.taskId,
      messages: transcriptToEntries(detail.items),
      plan: detail.plan,
      fileChanges: detail.fileChanges,
      workspacePath: detail.workspacePath ?? get().workspacePath,
      selectedDiff: detail.fileChanges[0]?.path ?? null,
      agentState: "idle",
      banner: null,
      proposal: null,
      attachments: [],
    }),
  newTask: (nextWorkspace) =>
    set({
      view: "chat",
      conversationId: null,
      taskId: null,
      messages: [],
      plan: [],
      fileChanges: [],
      selectedDiff: null,
      agentState: "idle",
      banner: null,
      permission: null,
      proposal: null,
      attachments: [],
      workspacePath: nextWorkspace === undefined ? get().desktopPath : nextWorkspace,
    }),
  pushUser: (content) =>
    set((state) => ({
      messages: [...state.messages, { id: crypto.randomUUID(), kind: "user", content }],
      banner: null,
    })),
  beginRun: (conversationId, taskId) =>
    set({
      conversationId,
      taskId,
      agentState: "thinking",
      plan: [],
      fileChanges: [],
      selectedDiff: null,
    }),
  setInspector: (inspectorTab) => set({ inspectorTab, inspectorOpen: true }),
  toggleInspector: () => set((state) => ({ inspectorOpen: !state.inspectorOpen })),
  selectDiff: (selectedDiff) => set({ selectedDiff, inspectorTab: "files", inspectorOpen: true }),
  applyEvent: (event) => {
    const state = get();
    const watch = state.automationWatch;
    if (watch && event.kind === "state" && (event.state === "completed" || event.state === "failed" || event.state === "cancelled")) {
      useUi.getState().finishAutomation(watch.id, event.state === "completed");
      if (watch.background) {
        if (event.state === "completed") useUi.getState().setNotice(translate(useUi.getState().language, "Automation finished."));
        set({ automationWatch: null, agentState: "idle", statusDetail: null, permission: null });
        return;
      }
      set({ automationWatch: null });
    }
    if (get().automationWatch?.background) return;
    if (event.kind === "state") {
      const proposal = state.proposal;
      if (proposal?.phase === "drafting" && (event.state === "completed" || event.state === "failed" || event.state === "cancelled")) {
        if (event.state === "completed") {
          const last = [...state.messages].reverse().find((entry) => entry.kind === "assistant");
          const text = last?.kind === "assistant" ? last.content.trim() : "";
          set({
            agentState: event.state,
            statusDetail: event.detail ?? null,
            permission: null,
            proposal: {
              ...proposal,
              phase: "ready",
              text: text || translate(useUi.getState().language, "{goal}\nStay inside this scope. Do not delete anything without approval.", { goal: proposal.goal }),
              editing: false,
            },
          });
          return;
        }
        set({
          agentState: event.state,
          statusDetail: event.detail ?? null,
          permission: null,
          proposal: null,
        });
        return;
      }
      set({
        agentState: event.state,
        statusDetail: event.detail ?? null,
        permission: event.state === "waiting_permission" ? state.permission : null,
      });
      return;
    }
    if (event.kind === "token") {
      const messages = [...state.messages];
      const last = messages[messages.length - 1];
      if (last?.kind === "assistant" && last.streaming) {
        messages[messages.length - 1] = { ...last, content: last.content + event.text };
      } else {
        messages.push({ id: crypto.randomUUID(), kind: "assistant", content: event.text, streaming: true });
      }
      set({ messages, agentState: "thinking" });
      return;
    }
    if (event.kind === "assistant_done") {
      const messages = [...state.messages];
      const last = messages[messages.length - 1];
      if (last?.kind === "assistant" && last.streaming) {
        if (!event.content.trim()) messages.pop();
        else messages[messages.length - 1] = { ...last, content: event.content, streaming: false };
      } else if (event.content.trim()) {
        messages.push({ id: crypto.randomUUID(), kind: "assistant", content: event.content });
      }
      const drafted = state.proposal?.phase === "drafting" && event.content.trim() ? event.content.trim() : null;
      set({
        messages,
        proposal: drafted && state.proposal ? { ...state.proposal, text: drafted } : state.proposal,
      });
      return;
    }
    if (event.kind === "tool_started") {
      set({
        messages: [
          ...state.messages,
          {
            id: event.id,
            kind: "tool",
            name: event.name,
            arguments: event.arguments,
            risk: event.risk,
            status: "running",
            output: "",
          },
        ],
      });
      return;
    }
    if (event.kind === "tool_finished") {
      const status: ToolStatus = wasDenied(event.content) ? "denied" : event.success ? "ok" : "error";
      set({
        messages: state.messages.map((entry) =>
          entry.kind === "tool" && entry.id === event.id ? { ...entry, status, output: event.content } : entry,
        ),
      });
      return;
    }
    if (event.kind === "plan") {
      set({ plan: event.steps, inspectorOpen: true, inspectorTab: "plan" });
      return;
    }
    if (event.kind === "permission") {
      set({ permission: event.request, agentState: "waiting_permission" });
      return;
    }
    if (event.kind === "file_change") {
      const existing = state.fileChanges.filter((change) => change.path !== event.change.path);
      set({
        fileChanges: [...existing, event.change],
        selectedDiff: event.change.path,
        inspectorTab: "files",
        inspectorOpen: true,
      });
      return;
    }
    set({ banner: event.message });
  },
}));

export function isRunning(state: AgentState): boolean {
  return state === "thinking" || state === "executing_tool" || state === "waiting_permission" || state === "verifying";
}
