import { create } from "zustand";

import {
  BUILTIN_SKILLS,
  CAPABILITIES,
  normalizeRunMode,
  type Approval,
  type Automation,
  type Effort,
  type Language,
  type Personality,
  type RunMode,
  type Skill,
} from "../lib/catalog";

const KEY = "orca-codex-ui";

interface Persisted {
  sidebarOpen: boolean;
  taskOpen: boolean;
  effort: Effort;
  approval: Approval;
  environment: "local" | "worktree";
  runMode: RunMode;
  personality: Personality;
  fontScale: number;
  pinned: string[];
  archived: string[];
  goals: Record<string, string>;
  skills: Skill[];
  enabledPlugins: string[];
  automations: Automation[];
  approvalMigrated: boolean;
  language: Language;
}

const defaults: Persisted = {
  sidebarOpen: true,
  taskOpen: false,
  effort: "medium",
  approval: "default",
  environment: "local",
  runMode: "mission",
  personality: "pragmatic",
  fontScale: 1,
  pinned: [],
  archived: [],
  goals: {},
  skills: [],
  enabledPlugins: CAPABILITIES.map((item) => item.id),
  automations: [],
  approvalMigrated: true,
  language: "en",
};

function load(): Persisted {
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return defaults;
    const stored = JSON.parse(raw) as Partial<Persisted>;
    const parsed = { ...defaults, ...stored };
    parsed.runMode = normalizeRunMode(String(parsed.runMode));
    if (!stored.approvalMigrated && stored.approval === "auto") {
      parsed.approval = "default";
      parsed.approvalMigrated = true;
      localStorage.setItem(KEY, JSON.stringify(parsed));
    }
    parsed.approvalMigrated = true;
    if (parsed.language !== "en" && parsed.language !== "ko") parsed.language = "en";
    const known = new Set(CAPABILITIES.map((item) => item.id));
    if (!parsed.enabledPlugins.some((id) => known.has(id) && id !== "documents")) {
      parsed.enabledPlugins = defaults.enabledPlugins;
    }
    return parsed;
  } catch {
    return defaults;
  }
}

interface UiStore extends Persisted {
  paletteOpen: boolean;
  findOpen: boolean;
  findQuery: string;
  findIndex: number;
  notice: string | null;
  modeSynced: boolean;
  setPalette: (open: boolean) => void;
  setFind: (open: boolean, query?: string) => void;
  setFindIndex: (index: number) => void;
  setNotice: (notice: string | null) => void;
  patch: (partial: Partial<Persisted>) => void;
  togglePin: (id: string) => void;
  toggleArchive: (id: string) => void;
  setGoal: (key: string, goal: string) => void;
  moveGoal: (from: string, to: string) => void;
  saveSkill: (skill: Skill) => void;
  removeSkill: (id: string) => void;
  togglePlugin: (id: string) => void;
  saveAutomation: (automation: Automation) => void;
  removeAutomation: (id: string) => void;
  markAutomation: (id: string, lastRun: number) => void;
  finishAutomation: (id: string, succeeded: boolean) => void;
  markModeSynced: (runMode: RunMode) => void;
}

function persist(state: Persisted) {
  const next: Persisted = {
    sidebarOpen: state.sidebarOpen,
    taskOpen: state.taskOpen,
    effort: state.effort,
    approval: state.approval,
    environment: state.environment,
    runMode: state.runMode,
    personality: state.personality,
    fontScale: state.fontScale,
    pinned: state.pinned,
    archived: state.archived,
    goals: state.goals,
    skills: state.skills,
    enabledPlugins: state.enabledPlugins,
    automations: state.automations,
    approvalMigrated: true,
    language: state.language,
  };
  localStorage.setItem(KEY, JSON.stringify(next));
}

export const useUi = create<UiStore>((set, get) => ({
  ...load(),
  paletteOpen: false,
  findOpen: false,
  findQuery: "",
  findIndex: 0,
  notice: null,
  modeSynced: false,
  setPalette: (paletteOpen) => set({ paletteOpen }),
  setFind: (findOpen, query) => set({ findOpen, findQuery: query ?? (findOpen ? get().findQuery : ""), findIndex: 0 }),
  setFindIndex: (findIndex) => set({ findIndex }),
  setNotice: (notice) => set({ notice }),
  patch: (partial) => {
    const next = { ...get(), ...partial };
    persist(next);
    set(partial);
  },
  togglePin: (id) => {
    const pinned = get().pinned.includes(id) ? get().pinned.filter((item) => item !== id) : [id, ...get().pinned];
    get().patch({ pinned });
  },
  toggleArchive: (id) => {
    const archived = get().archived.includes(id) ? get().archived.filter((item) => item !== id) : [id, ...get().archived];
    get().patch({ archived });
  },
  setGoal: (key, goal) => get().patch({ goals: { ...get().goals, [key]: goal } }),
  moveGoal: (from, to) => {
    const goals = { ...get().goals };
    if (!goals[from]) return;
    goals[to] = goals[from];
    delete goals[from];
    get().patch({ goals });
  },
  saveSkill: (skill) => {
    const skills = get().skills.some((item) => item.id === skill.id)
      ? get().skills.map((item) => (item.id === skill.id ? skill : item))
      : [...get().skills, skill];
    get().patch({ skills });
  },
  removeSkill: (id) => get().patch({ skills: get().skills.filter((skill) => skill.id !== id) }),
  togglePlugin: (id) => {
    const enabledPlugins = get().enabledPlugins.includes(id)
      ? get().enabledPlugins.filter((item) => item !== id)
      : [...get().enabledPlugins, id];
    get().patch({ enabledPlugins });
  },
  saveAutomation: (automation) => {
    const automations = get().automations.some((item) => item.id === automation.id)
      ? get().automations.map((item) => (item.id === automation.id ? automation : item))
      : [...get().automations, automation];
    get().patch({ automations });
  },
  removeAutomation: (id) => get().patch({ automations: get().automations.filter((item) => item.id !== id) }),
  markAutomation: (id, lastRun) =>
    get().patch({
      automations: get().automations.map((item) => (item.id === id ? { ...item, lastRun, retryAfter: undefined } : item)),
    }),
  finishAutomation: (id, succeeded) => {
    const now = Date.now();
    get().patch({
      automations: get().automations.map((item) => {
        if (item.id !== id) return item;
        if (succeeded) return { ...item, lastRun: now, retryAfter: undefined };
        return { ...item, retryAfter: now + 5 * 60 * 1000 };
      }),
    });
  },
  markModeSynced: (runMode) => set({ modeSynced: true, runMode }),
}));

export function allSkills(custom: Skill[]): Skill[] {
  const names = new Set(custom.map((skill) => skill.name));
  return [...BUILTIN_SKILLS.filter((skill) => !names.has(skill.name)), ...custom];
}
