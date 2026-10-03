import { create } from "zustand";

import type { Settings } from "../types";

const fallback: Settings = {
  ollamaUrl: "http://127.0.0.1:11434",
  model: "",
  temperature: 0.2,
  contextLength: 8192,
  maxIterations: 40,
  autoApproveSafe: true,
  autoApproveFileEdits: true,
  terminalTimeoutMs: 120000,
  systemPrompt: "",
  mode: "mission",
  workspacePath: null,
};

interface SettingsStore {
  settings: Settings;
  defaultPrompt: string;
  setSettings: (settings: Settings, defaultPrompt?: string) => void;
  patch: (partial: Partial<Settings>) => void;
}

export const useSettings = create<SettingsStore>((set) => ({
  settings: fallback,
  defaultPrompt: "",
  setSettings: (settings, defaultPrompt) =>
    set((state) => ({
      settings,
      defaultPrompt: defaultPrompt ?? state.defaultPrompt,
    })),
  patch: (partial) => set((state) => ({ settings: { ...state.settings, ...partial } })),
}));
