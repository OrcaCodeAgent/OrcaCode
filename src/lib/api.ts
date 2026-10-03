import { invoke } from "@tauri-apps/api/core";

import { translate } from "./i18n";
import { useUi } from "../stores/ui";
import type {
  ConversationDetail,
  ConversationSummary,
  ProcessInfo,
  Settings,
  WorkspaceRecord,
} from "../types";

export interface SettingsPayload {
  settings: Settings;
  defaultPrompt: string;
}

export interface UndoReport {
  restored: string[];
  skipped: string[];
}

export function rawMessage(error: unknown): string {
  if (typeof error === "string" && error.trim()) return error;
  if (error instanceof Error && error.message.trim()) return error.message;
  return "Could not complete the request.";
}

export function undoSummary(report: UndoReport): string {
  const language = useUi.getState().language;
  if (report.skipped.length > 0) {
    return translate(language, "Restored {restored}. Left {skipped} folders in place because they were not empty.", {
      restored: report.restored.length,
      skipped: report.skipped.length,
    });
  }
  return report.restored.length > 0
    ? translate(language, "Restored {count} files.", { count: report.restored.length })
    : translate(language, "No file snapshot to restore.");
}

export function explain(error: unknown): string {
  return translate(useUi.getState().language, rawMessage(error));
}

export const api = {
  getSettings: () => invoke<SettingsPayload>("get_settings"),
  saveSettings: (settings: Settings) => invoke<Settings>("save_settings", { settings }),
  listWorkspaces: () => invoke<WorkspaceRecord[]>("list_workspaces"),
  rememberWorkspace: (path: string) => invoke<WorkspaceRecord>("remember_workspace", { path }),
  listConversations: () => invoke<ConversationSummary[]>("list_conversations"),
  getConversation: (id: string) => invoke<ConversationDetail | null>("get_conversation", { id }),
  removeConversation: (id: string) => invoke<void>("remove_conversation", { id }),
  renameConversation: (id: string, title: string) => invoke<void>("rename_conversation", { id, title }),
  workspaceDiff: (path: string) => invoke<string>("workspace_diff", { path }),
  createWorktree: (repo: string) => invoke<string>("create_worktree", { repo }),
  openExternalUrl: (url: string) => invoke<void>("open_external_url", { url }),
  ollamaStatus: () => invoke<{ online: boolean; message: string }>("ollama_status"),
  ollamaModels: () => invoke<string[]>("ollama_models"),
  ollamaPresent: () => invoke<boolean>("ollama_present"),
  pullOllamaModel: (name: string) => invoke<void>("pull_ollama_model", { name }),
  unloadOllamaModel: (name: string) => invoke<void>("unload_ollama_model", { name }),
  installOllama: () => invoke<string>("install_ollama"),
  bootstrapRuntime: () => invoke<{ online: boolean; model: string; models: string[] }>("bootstrap_runtime"),
  ensureOllamaServer: () => invoke<void>("ensure_ollama_server"),
  startTask: (request: {
    conversationId: string | null;
    goal: string;
    mode: string;
    workspacePath: string;
    approval?: string;
    effort?: string;
    instructions?: string;
    recordUser?: boolean;
  }) => invoke<{ conversationId: string; taskId: string }>("start_task", { request }),
  computerHome: () => invoke<string>("computer_home"),
  computerDesktop: () => invoke<string>("computer_desktop"),
  setRunsInBackground: (enabled: boolean) => invoke<void>("set_runs_in_background", { enabled }),
  cancelTask: () => invoke<void>("cancel_task"),
  respondPermission: (requestId: string, decision: "allow" | "once" | "deny") =>
    invoke<void>("respond_permission", { answer: { requestId, decision } }),
  undoTask: (taskId: string) => invoke<UndoReport>("undo_task", { taskId }),
  listProcesses: () => invoke<ProcessInfo[]>("list_processes"),
  readProcess: (id: string) => invoke<string>("read_process", { id }),
  stopProcess: (id: string) => invoke<string>("stop_process", { id }),
  accessibilityStatus: () => invoke<boolean>("accessibility_status"),
  openAccessibilitySettings: () => invoke<void>("open_accessibility_settings"),
};
