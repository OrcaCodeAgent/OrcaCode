import { useEffect, useState, type ReactNode } from "react";

import { api, explain } from "../lib/api";
import { useT } from "../lib/i18n";
import type { Language } from "../lib/catalog";
import { useSession } from "../stores/session";
import { useUi } from "../stores/ui";
import { useSettings } from "../stores/settings";
import type { Settings } from "../types";
import { ModelLibrary } from "./ModelLibrary";
import { UpdateSection } from "./UpdateSection";

const sections = ["General", "Model", "Permissions", "Computer", "Shortcuts"] as const;

export function SettingsView() {
  const settings = useSettings((state) => state.settings);
  const defaultPrompt = useSettings((state) => state.defaultPrompt);
  const setSettings = useSettings((state) => state.setSettings);
  const setBanner = useSession((state) => state.setBanner);
  const setOllama = useSession((state) => state.setOllama);
  const personality = useUi((state) => state.personality);
  const language = useUi((state) => state.language);
  const patchUi = useUi((state) => state.patch);
  const t = useT();
  const [draft, setDraft] = useState(settings);
  const [section, setSection] = useState<(typeof sections)[number]>("General");
  const [accessibility, setAccessibility] = useState<boolean | null>(null);

  useEffect(() => setDraft(settings), [settings]);
  useEffect(() => {
    void api.accessibilityStatus().then(setAccessibility).catch(() => setAccessibility(false));
    void api
      .ensureOllamaServer()
      .then(async () => {
        const status = await api.ollamaStatus();
        const models = status.online ? await api.ollamaModels().catch(() => []) : [];
        setOllama(status.online, status.message, models);
      })
      .catch((error) => setBanner(explain(error)));
  }, [setBanner, setOllama]);

  async function save(next: Settings = draft) {
    try {
      const saved = await api.saveSettings(next);
      setSettings(saved);
      const status = await api.ollamaStatus();
      const models = status.online ? await api.ollamaModels().catch(() => []) : [];
      setOllama(status.online, status.message, models);
      setBanner(t("Settings saved."));
    } catch (error) {
      setBanner(explain(error));
    }
  }

  return (
    <div className="flex min-h-0 flex-1">
      <div className="w-44 shrink-0 border-r border-line p-3">
        <div className="px-2 py-2 text-sm font-medium">{t("Settings")}</div>
        {sections.map((item) => (
          <button key={item} className={`block w-full rounded-md px-2 py-1.5 text-left text-sm ${section === item ? "bg-elev" : "text-muted"}`} onClick={() => setSection(item)}>
            {t(item)}
          </button>
        ))}
      </div>
      <div className="scroll-thin min-w-0 flex-1 overflow-auto px-8 py-6">
        <div className="mx-auto max-w-xl">
          {section === "General" ? (
            <div className="space-y-4">
              <h1 className="text-xl font-medium">{t("General")}</h1>
              <p className="text-sm text-muted">{t("Conversations and settings stay on this device. There is no cloud account.")}</p>
              <p className="text-sm text-muted">{t("Ollama starts when the app opens. The default model is qwen2.5-coder:7b. If an automation is enabled, closing the window keeps the app running in the background.")}</p>
              <Field label={t("Language")}>
                <select className="field" value={language} onChange={(event) => patchUi({ language: event.target.value as Language })}>
                  <option value="en">{t("English")}</option>
                  <option value="ko">{t("Korean")}</option>
                </select>
              </Field>
              <UpdateSection />
              <Field label={t("Personality")}>
                <select className="field" value={personality} onChange={(event) => patchUi({ personality: event.target.value as "pragmatic" | "friendly" })}>
                  <option value="pragmatic">Pragmatic</option>
                  <option value="friendly">Friendly</option>
                </select>
              </Field>
              <p className="text-xs text-muted">{t("You can also change this with /personality. Pragmatic stays short. Friendly adds one line of reason.")}</p>
            </div>
          ) : null}
          {section === "Model" ? (
            <div className="space-y-4">
              <h1 className="text-xl font-medium">{t("Model")}</h1>
              <ModelLibrary />
              <Field label="Ollama URL">
                <input className="field" value={draft.ollamaUrl} onChange={(event) => setDraft({ ...draft, ollamaUrl: event.target.value })} />
              </Field>
              {draft.ollamaUrl && !/localhost|127\.0\.0\.1|\[::1\]/.test(draft.ollamaUrl) ? (
                <p className="text-xs text-warn">{t("This address is not Ollama on this device. Conversations and files are sent there.")}</p>
              ) : null}
              <div className="grid grid-cols-2 gap-3">
                <Field label="Temperature">
                  <input className="field" type="number" step="0.1" value={draft.temperature} onChange={(event) => setDraft({ ...draft, temperature: Number(event.target.value) })} />
                </Field>
                <p className="col-span-2 text-xs text-muted">{t("While a task runs, the composer effort level overrides the temperature.")}</p>
                <Field label="Context length">
                  <input className="field" type="number" value={draft.contextLength} onChange={(event) => setDraft({ ...draft, contextLength: Number(event.target.value) })} />
                </Field>
                <Field label="Max iterations">
                  <input className="field" type="number" value={draft.maxIterations} onChange={(event) => setDraft({ ...draft, maxIterations: Number(event.target.value) })} />
                </Field>
                <Field label="Terminal timeout (ms)">
                  <input className="field" type="number" value={draft.terminalTimeoutMs} onChange={(event) => setDraft({ ...draft, terminalTimeoutMs: Number(event.target.value) })} />
                </Field>
              </div>
              <Field label={t("System prompt")}>
                <textarea className="field h-40 font-mono text-xs" value={draft.systemPrompt || defaultPrompt} onChange={(event) => setDraft({ ...draft, systemPrompt: event.target.value })} />
              </Field>
            </div>
          ) : null}
          {section === "Permissions" ? (
            <div className="space-y-3 text-sm">
              <h1 className="text-xl font-medium">{t("Permissions")}</h1>
              <p className="text-muted">{t("The composer's Default, Auto, and Full access choices override this for the current run. These values are the fallback.")}</p>
              <label className="flex items-center gap-2">
                <input type="checkbox" checked={draft.autoApproveSafe} onChange={(event) => setDraft({ ...draft, autoApproveSafe: event.target.checked })} />
                {t("Auto-approve reads and lookups")}
              </label>
              <label className="flex items-center gap-2">
                <input type="checkbox" checked={draft.autoApproveFileEdits} onChange={(event) => setDraft({ ...draft, autoApproveFileEdits: event.target.checked })} />
                {t("Auto-approve edits, installs, and git add inside the working folder")}
              </label>
              <p className="text-xs text-muted">{t("Commands that wipe the home directory or a system path are blocked in every mode.")}</p>
            </div>
          ) : null}
          {section === "Computer" ? (
            <div className="space-y-3 text-sm">
              <h1 className="text-xl font-medium">{t("Computer use")}</h1>
              <p className="text-muted">{t("When Computer is on, screenshots and Accessibility can control other apps. Screen Recording and Accessibility permissions are required.")}</p>
              <div className="flex items-center gap-3">
                <span className="text-muted">{t("Accessibility: {state}", { state: accessibility ? t("Allowed") : t("Needed") })}</span>
                <button className="rounded-md border border-line px-3 py-1.5" onClick={() => void api.openAccessibilitySettings()}>
                  {t("Open System Settings")}
                </button>
              </div>
              <p className="text-xs text-muted">{t("Web pages are read as public text, not through a signed-in browser. This local app has no cloud remote control and no Chrome extension.")}</p>
            </div>
          ) : null}
          {section === "Shortcuts" ? <Shortcuts /> : null}
          {section !== "Shortcuts" && section !== "General" && section !== "Computer" ? (
            <div className="mt-5 flex gap-2">
              <button className="rounded-full bg-white px-4 py-2 text-sm text-black" onClick={() => void save()}>
                {t("Save")}
              </button>
              {section === "Model" ? (
                <button
                  className="rounded-full border border-line px-4 py-2 text-sm"
                  onClick={() => {
                    const next = { ...draft, systemPrompt: "" };
                    setDraft(next);
                    void save(next);
                  }}
                >
                  {t("Reset prompt")}
                </button>
              ) : null}
            </div>
          ) : null}
        </div>
      </div>
    </div>
  );
}

function Shortcuts() {
  const t = useT();
  const rows = [
    ["New chat", "⌘N"],
    ["Command menu", "⌘K"],
    ["Settings", "⌘,"],
    ["Open folder", "⌘O"],
    ["Sidebar", "⌘B"],
    ["Task panel", "⌘J"],
    ["Find in chat", "⌘F"],
    ["Find next", "⌘G"],
    ["Terminal tab", "⌃`"],
    ["Text size", "⌘+ / ⌘- / ⌘0"],
    ["Approve", "Enter"],
    ["Deny", "Esc"],
  ];
  return (
    <div>
      <h1 className="text-xl font-medium">{t("Shortcuts")}</h1>
      <div className="mt-4 divide-y divide-line border-y border-line">
        {rows.map(([label, keys]) => (
          <div key={label} className="flex items-center py-2 text-sm">
            <span>{t(label)}</span>
            <span className="ml-auto text-muted">{keys}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="block text-sm">
      <div className="mb-1 text-muted">{label}</div>
      {children}
    </label>
  );
}
