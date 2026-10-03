import { useState } from "react";

import type { Automation, Cadence } from "../lib/catalog";
import { useT } from "../lib/i18n";
import { useSession } from "../stores/session";
import { useUi } from "../stores/ui";

export function AutomationsView() {
  const automations = useUi((state) => state.automations);
  const saveAutomation = useUi((state) => state.saveAutomation);
  const removeAutomation = useUi((state) => state.removeAutomation);
  const workspacePath = useSession((state) => state.workspacePath);
  const [name, setName] = useState("");
  const [prompt, setPrompt] = useState("");
  const [cadence, setCadence] = useState<Cadence>("daily");
  const t = useT();

  return (
    <div className="scroll-thin flex-1 overflow-auto px-8 py-6">
      <div className="mx-auto max-w-xl">
        <h1 className="text-xl font-medium">{t("Automations")}</h1>
        <p className="mt-2 text-sm text-muted">{t("While the app is open, the same request runs again. A Friday job, or something you want to trigger after dropping in photos, can live here.")}</p>
        <form
          className="mt-5 space-y-3"
          onSubmit={(event) => {
            event.preventDefault();
            if (!name.trim() || !prompt.trim() || !workspacePath) return;
            const automation: Automation = {
              id: crypto.randomUUID(),
              name: name.trim(),
              prompt: prompt.trim(),
              workspacePath,
              cadence,
              enabled: true,
              lastRun: 0,
            };
            saveAutomation(automation);
            setName("");
            setPrompt("");
          }}
        >
          <input className="field" placeholder={t("Name")} value={name} onChange={(event) => setName(event.target.value)} />
          <textarea className="field h-24" placeholder={t("Request to run")} value={prompt} onChange={(event) => setPrompt(event.target.value)} />
          <select className="field" value={cadence} onChange={(event) => setCadence(event.target.value as Cadence)}>
            <option value="hourly">{t("Hourly")}</option>
            <option value="daily">{t("Daily")}</option>
            <option value="weekly">{t("Weekly")}</option>
            <option value="friday">{t("Every Friday")}</option>
          </select>
          <button className="rounded-full bg-white px-4 py-2 text-sm text-black disabled:opacity-40" disabled={!workspacePath}>
            {workspacePath ? t("Add automation") : t("Opening this computer")}
          </button>
        </form>
        <div className="mt-6 space-y-2">
          {automations.length === 0 ? <p className="text-sm text-muted">{t("No automations yet.")}</p> : null}
          {automations.map((automation) => (
            <div key={automation.id} className="rounded-xl border border-line bg-panel-2 px-3 py-3">
              <div className="flex items-center gap-2">
                <div className="font-medium">{automation.name}</div>
                <span className="text-xs text-muted">{cadenceLabel(automation.cadence, t)}</span>
                <button
                  className="ml-auto text-xs text-muted"
                  onClick={() => saveAutomation({ ...automation, enabled: !automation.enabled })}
                >
                  {automation.enabled ? t("Pause") : t("Resume")}
                </button>
                <button className="text-xs text-muted" onClick={() => removeAutomation(automation.id)}>
                  {t("Delete")}
                </button>
              </div>
              <div className="mt-1 truncate text-sm text-muted">{automation.prompt}</div>
              <div className="mt-1 truncate text-xs text-muted">{automation.workspacePath}</div>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

function cadenceLabel(cadence: Cadence, t: (text: string) => string): string {
  if (cadence === "hourly") return t("Hourly");
  if (cadence === "weekly") return t("Weekly");
  if (cadence === "friday") return t("Every Friday");
  return t("Daily");
}
