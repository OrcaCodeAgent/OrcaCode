export type Effort = "low" | "medium" | "high" | "xhigh";
export type Approval = "default" | "auto" | "full";
export type Personality = "pragmatic" | "friendly";
export type RunMode = "ask" | "do" | "mission";
export type Language = "en" | "ko";
export type Cadence = "hourly" | "daily" | "weekly" | "friday";

export interface Skill {
  id: string;
  name: string;
  description: string;
  body: string;
  builtin?: boolean;
}

export interface Automation {
  id: string;
  name: string;
  prompt: string;
  workspacePath: string;
  cadence: Cadence;
  enabled: boolean;
  lastRun: number;
  retryAfter?: number;
}

export interface Plugin {
  id: string;
  name: string;
  description: string;
  instruction: string;
}

export const CAPABILITIES: Plugin[] = [
  {
    id: "computer",
    name: "Computer",
    description: "Files, apps, installs, and the screen",
    instruction:
      "Computer: move, rename, and organize files. Open apps and install only what the user asked for. Prefer file tools over clicking. Do not delete unless the approved plan says so.",
  },
  {
    id: "documents",
    name: "Documents",
    description: "PDF, Word, Excel, slides, images, and text",
    instruction:
      "Documents: read and produce PDF, Word, Excel, slides, images, and text. Use a local converter when the format is not plain text. Keep the original files. State every column or page you change.",
  },
  {
    id: "web",
    name: "Web",
    description: "Search, compare, and summarize",
    instruction:
      "Web: search, open http(s) pages, compare sources, and write the result down. Do not claim a page was opened unless open_url ran.",
  },
  {
    id: "code",
    name: "Code",
    description: "Code, terminal, Git, build, and test",
    instruction:
      "Code: read before editing, use the terminal and Git, then build or test. Do not reset or force-push.",
  },
  {
    id: "automations",
    name: "Automations",
    description: "Run the same job again",
    instruction:
      "Automations: if the user wants a repeated job, say which schedule fits: hourly, daily, weekly, or Friday. The app stores that schedule. Do not start a hidden background process.",
  },
];

export const PLUGINS = CAPABILITIES;

export const BUILTIN_SKILLS: Skill[] = [
  {
    id: "review",
    name: "review",
    description: "Review changes before a commit",
    builtin: true,
    body: "Review the uncommitted diff. List real risks, missing tests, and anything that should be reverted. Do not edit files.",
  },
  {
    id: "skill-creator",
    name: "skill-creator",
    description: "Turn a repeated job into a skill",
    builtin: true,
    body: "Help the user define a reusable skill. Ask what should trigger it, then write a name, a one-line description, and the instructions. Do not save secrets into the skill.",
  },
];

export const SLASH = [
  { id: "goal", label: "Goal", detail: "Set the goal this thread keeps following" },
  { id: "model", label: "Model", detail: "Choose the model and reasoning effort" },
  { id: "fast", label: "Fast", detail: "Lower reasoning for a short task" },
  { id: "chat", label: "Ask", detail: "Explain only. Do not change files" },
  { id: "review", label: "Review", detail: "Review uncommitted changes" },
  { id: "status", label: "Status", detail: "Show the thread and context status" },
  { id: "personality", label: "Personality", detail: "Change how answers are written" },
] as const;

export function effortLabel(effort: Effort): string {
  if (effort === "low") return "Low";
  if (effort === "high") return "High";
  if (effort === "xhigh") return "Extra high";
  return "Medium";
}

export function approvalLabel(approval: Approval): string {
  if (approval === "default") return "Default";
  if (approval === "full") return "Full access";
  return "Auto";
}

export function runModeLabel(mode: RunMode): string {
  if (mode === "ask") return "Ask";
  if (mode === "do") return "Do";
  return "Mission";
}

export function normalizeRunMode(value: string): RunMode {
  if (value === "ask") return "ask";
  if (value === "do" || value === "plan") return "do";
  return "mission";
}

export function previewInstructions(mode: "do" | "mission", goal: string): string {
  return [
    "Preview only. Do not edit, move, delete, install, or commit.",
    "You may list and read enough to learn the real scope.",
    "Finish with one short paragraph in the language named by the instructions.",
    "Name what you will do, what you will not delete, and the order.",
    mode === "do" ? "After approval this runs as one short job." : "After approval this runs as a multi-step mission.",
    `The user asked:\n${goal}`,
  ].join("\n");
}

export function approvedPlan(text: string): string {
  return `The user approved this plan. Follow it and do not widen it.\n\n${text}`;
}

export function samePath(left: string | null, right: string | null): boolean {
  if (!left || !right) return false;
  return left.replace(/\/+$/, "") === right.replace(/\/+$/, "");
}

export function isPlainChat(path: string | null, desktop: string | null): boolean {
  if (!path) return true;
  return samePath(path, desktop);
}

export function scopeInstructions(plain: boolean, folderLabel: string): string {
  if (plain) {
    return [
      "Scope: plain chat. The workspace is the Desktop folder.",
      "Save new files on the Desktop unless the user named another folder.",
      "Read a folder only when the user named it. Do not reorganize the whole computer.",
      "This is not a code project unless the user points at one.",
    ].join("\n");
  }
  return [
    `Scope: folder "${folderLabel}".`,
    "Read and write inside this folder. Leave other folders alone unless the user named a file there.",
    "New files belong in this folder.",
  ].join("\n");
}

export function previewEvidence(entries: { kind: string; name?: string; output?: string; status?: string }[]): string {
  const tools = entries.filter((entry) => entry.kind === "tool" && entry.status !== "running").slice(-12);
  if (tools.length === 0) return "";
  const lines: string[] = [];
  let used = 0;
  for (const tool of tools) {
    const body = (tool.output ?? "").slice(0, 1_200);
    if (used + body.length > 8_000) break;
    used += body.length;
    lines.push(`- ${tool.name ?? "tool"}: ${body}`);
  }
  if (lines.length === 0) return "";
  return `Findings already gathered. Reuse them instead of repeating the same reads.\n${lines.join("\n")}`;
}

export function automationDue(automation: Automation, now: number): boolean {
  if (!automation.enabled || !automation.workspacePath) return false;
  const elapsed = now - automation.lastRun;
  if (automation.retryAfter && now < automation.retryAfter) return false;
  if (automation.cadence === "friday") {
    return new Date(now).getDay() === 5 && elapsed >= 20 * 60 * 60 * 1000;
  }
  return elapsed >= cadenceMs(automation.cadence);
}

const IMAGE_EXT = new Set(["png", "jpg", "jpeg", "gif", "webp", "heic", "tif", "tiff"]);
const SHEET_EXT = new Set(["csv", "tsv", "xlsx", "xls", "numbers"]);
const DOC_EXT = new Set(["pdf", "doc", "docx", "ppt", "pptx", "txt", "md", "rtf", "pages"]);
const CODE_EXT = new Set(["ts", "tsx", "js", "jsx", "rs", "py", "go", "java", "c", "cpp", "h", "css", "html", "json", "toml"]);

export function intentSuggestions(paths: string[]): { label: string; prompt: string }[] {
  if (paths.length === 0) return [];
  const exts = paths.map((path) => path.split(".").pop()?.toLowerCase() ?? "");
  const every = (set: Set<string>) => exts.every((ext) => set.has(ext));
  const some = (set: Set<string>) => exts.some((ext) => set.has(ext));
  if (every(DOC_EXT) && exts.every((ext) => ext === "pdf" || ext === "doc" || ext === "docx")) {
    return [
      { label: "Summarize", prompt: "Summarize this document." },
      { label: "Translate", prompt: "Translate this document into the selected language." },
      { label: "New document", prompt: "Create a new document from this one." },
    ];
  }
  if (every(IMAGE_EXT)) {
    return [
      { label: "Sort by date", prompt: "Organize these photos by date. Do not delete the originals." },
      { label: "Find duplicates", prompt: "Find duplicates among these photos. Show a list and do not delete anything." },
    ];
  }
  if (some(SHEET_EXT)) {
    return [
      { label: "Make a table", prompt: "Turn this data into a clear table." },
      { label: "Chart data", prompt: "Read this data and arrange it so it can become a chart." },
    ];
  }
  if (some(CODE_EXT)) {
    return [
      { label: "Fix errors", prompt: "Fix the errors in this project." },
      { label: "Explain", prompt: "Explain what this code does. Do not change files." },
    ];
  }
  return [
    { label: "Organize", prompt: "Organize these files by type. Do not delete anything." },
    { label: "Explain", prompt: "Explain what these files are." },
  ];
}

export function cadenceMs(cadence: Cadence): number {
  if (cadence === "hourly") return 60 * 60 * 1000;
  if (cadence === "weekly" || cadence === "friday") return 7 * 24 * 60 * 60 * 1000;
  return 24 * 60 * 60 * 1000;
}

export function buildInstructions(input: {
  text: string;
  personality: Personality;
  effort: Effort;
  goal: string;
  skills: Skill[];
  enabledPlugins: string[];
  scope?: string;
  language?: Language;
}): string {
  const lines = [
    input.language === "ko" ? "Reply in Korean." : "Reply in English.",
    input.scope ?? "",
    input.personality === "friendly"
      ? "Personality: warm and clear. Explain the decision in a sentence, then do the work."
      : "Personality: terse and pragmatic. Lead with the result.",
  ].filter(Boolean);
  if (input.effort === "low") lines.push("Reasoning effort is low. Take the shortest safe path.");
  if (input.effort === "high") lines.push("Reasoning effort is high. Check edge cases before you finish.");
  if (input.effort === "xhigh") lines.push("Reasoning effort is extra high. Inspect the result before you stop.");
  if (input.goal.trim()) lines.push(`Persistent goal, keep working toward it:\n${input.goal.trim()}`);
  for (const skill of input.skills) {
    if (input.text.includes(`$${skill.name}`)) {
      lines.push(`Skill $${skill.name}: ${skill.description}\n${skill.body}`);
    }
  }
  for (const capability of CAPABILITIES) {
    if (!input.enabledPlugins.includes(capability.id)) continue;
    const named = input.text.includes(`@${capability.name}`);
    lines.push(named ? `Use ${capability.name} for this request.\n${capability.instruction}` : capability.instruction);
  }
  return lines.join("\n\n");
}
