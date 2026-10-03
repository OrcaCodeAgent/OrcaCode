import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";

import { api, explain } from "../lib/api";
import { useT } from "../lib/i18n";
import { useSession } from "../stores/session";
import { useSettings } from "../stores/settings";
import { useUi } from "../stores/ui";

const catalog = [
  { name: "qwen2.5-coder:7b", detail: "Default coding model" },
  { name: "qwen2.5-coder:14b", detail: "Coding model for longer tasks" },
  { name: "qwen2.5:7b", detail: "General tasks" },
  { name: "llama3.1:8b", detail: "General conversation model" },
  { name: "gemma2:9b", detail: "Light general model" },
  { name: "deepseek-coder-v2:16b", detail: "For reading code" },
  { name: "mistral:7b", detail: "Fast general model" },
  { name: "phi3:3.8b", detail: "For a smaller machine" },
  { name: "qwen2.5vl:7b", detail: "Model that can see the screen and images" },
  { name: "llava:7b", detail: "For describing images" },
  { name: "moondream:latest", detail: "Light image model" },
];

interface Progress {
  kind: string;
  status: string;
  completed: number;
  total: number;
  done: boolean;
  error?: string | null;
}

export function ModelLibrary() {
  const models = useSession((state) => state.models);
  const online = useSession((state) => state.ollamaOnline);
  const message = useSession((state) => state.ollamaMessage);
  const setOllama = useSession((state) => state.setOllama);
  const setBanner = useSession((state) => state.setBanner);
  const setNotice = useUi((state) => state.setNotice);
  const settings = useSettings((state) => state.settings);
  const t = useT();
  const patch = useSettings((state) => state.patch);
  const setSettings = useSettings((state) => state.setSettings);
  const [present, setPresent] = useState<boolean | null>(null);
  const [custom, setCustom] = useState("");
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<Progress | null>(null);

  async function refresh() {
    const status = await api.ollamaStatus();
    const installed = status.online ? await api.ollamaModels().catch(() => []) : [];
    setOllama(status.online, status.message, installed);
    setPresent(await api.ollamaPresent().catch(() => false));
  }

  useEffect(() => {
    void refresh().catch((error) => setBanner(explain(error)));
    let unlisten: (() => void) | undefined;
    void listen<Progress>("ollama-progress", (event) => setProgress(event.payload))
      .then((stop) => {
        unlisten = stop;
      })
      .catch(() => undefined);
    return () => unlisten?.();
  }, [setBanner, setOllama]);

  async function choose(name: string) {
    const next = { ...settings, model: name };
    patch({ model: name });
    try {
      setSettings(await api.saveSettings(next));
    } catch (error) {
      setBanner(explain(error));
    }
  }

  async function pull(name: string) {
    const model = name.trim();
    if (!model || busy) return;
    setBusy(true);
    setProgress({ kind: "model", status: `Downloading ${model}`, completed: 0, total: 0, done: false });
    try {
      await api.pullOllamaModel(model);
      await refresh();
      await choose(model);
      setCustom("");
    } catch (error) {
      setBanner(explain(error));
    } finally {
      setBusy(false);
    }
  }

  async function unload(name: string) {
    if (busy) return;
    setBusy(true);
    try {
      await api.unloadOllamaModel(name);
      setNotice(t("Unloaded {name} from memory.", { name }));
    } catch (error) {
      setBanner(explain(error));
    } finally {
      setBusy(false);
    }
  }

  async function installApp() {
    if (busy) return;
    setBusy(true);
    setProgress({ kind: "app", status: "Preparing Ollama", completed: 0, total: 0, done: false });
    try {
      const result = await api.installOllama();
      setNotice(result);
      for (let attempt = 0; attempt < 8; attempt += 1) {
        await new Promise((resolve) => window.setTimeout(resolve, 1500));
        await refresh();
        if (useSession.getState().ollamaOnline) break;
      }
    } catch (error) {
      setBanner(explain(error));
    } finally {
      setBusy(false);
    }
  }

  const ratio = progress && progress.total > 0 ? Math.min(100, Math.round((progress.completed / progress.total) * 100)) : 0;
  const installedNames = new Set(models);

  return (
    <div className="space-y-4">
      <div className="rounded-xl border border-line bg-panel-2 px-3 py-3 text-sm">
        <div className="flex items-center gap-2">
          <span className={`h-2 w-2 rounded-full ${online ? "bg-ok" : "bg-danger"}`} />
          <span>{online ? t("Ollama is running.") : present ? t("Ollama is installed but not running.") : t("Ollama is not installed.")}</span>
        </div>
        {!online ? <p className="mt-2 text-xs text-muted">{t(message)}</p> : null}
        {!online ? (
          <button className="mt-3 rounded-full bg-white px-3 py-1.5 text-sm text-black disabled:opacity-40" disabled={busy} onClick={() => void installApp()}>
            {present ? t("Start Ollama") : t("Install Ollama")}
          </button>
        ) : null}
      </div>
      {progress && !progress.done ? (
        <div>
          <div className="mb-1 flex justify-between text-xs text-muted">
            <span>{t(progress.status)}</span>
            {progress.total > 0 ? <span>{ratio}%</span> : null}
          </div>
          <div className="h-1.5 overflow-hidden rounded-full bg-elev">
            <div className="h-full bg-white" style={{ width: progress.total > 0 ? `${ratio}%` : "30%" }} />
          </div>
        </div>
      ) : null}
      <div>
        <div className="mb-2 text-sm text-muted">{t("Installed models")}</div>
        {models.length === 0 ? <p className="text-sm text-muted">{t("No models installed yet.")}</p> : null}
        <div className="space-y-1">
          {models.map((model) => (
            <button
              key={model}
              className={`flex w-full items-center rounded-lg px-3 py-2 text-left text-sm ${settings.model === model ? "bg-elev" : "hover:bg-elev"}`}
              onClick={() => void choose(model)}
            >
              <span className="truncate">{model}</span>
              {settings.model === model ? <span className="ml-auto text-xs text-muted">{t("In use")}</span> : null}
            </button>
          ))}
        </div>
      </div>
      <div>
        <div className="mb-2 text-sm text-muted">{t("Models to install")}</div>
        <div className="space-y-1">
          {catalog.map((item) => {
            const installed = [...installedNames].some((name) => name === item.name || name.startsWith(item.name));
            return (
              <div key={item.name} className="flex items-center gap-3 rounded-lg px-3 py-2">
                <div className="min-w-0 flex-1">
                  <div className="truncate text-sm">{item.name}</div>
                  <div className="text-xs text-muted">{t(item.detail)}</div>
                </div>
                {installed ? (
                  <button
                    className="rounded-full border border-line px-3 py-1 text-xs disabled:opacity-40"
                    disabled={!online || busy}
                    onClick={() => void unload(item.name)}
                  >
                    {t("Unload from memory")}
                  </button>
                ) : (
                  <button className="rounded-full border border-line px-3 py-1 text-xs disabled:opacity-40" disabled={!online || busy} onClick={() => void pull(item.name)}>
                    {t("Install")}
                  </button>
                )}
              </div>
            );
          })}
        </div>
        <form
          className="mt-3 flex gap-2"
          onSubmit={(event) => {
            event.preventDefault();
            void pull(custom);
          }}
        >
          <input className="field" placeholder={t("Model name, for example qwen2.5-coder:7b")} value={custom} onChange={(event) => setCustom(event.target.value)} />
          <button className="shrink-0 rounded-full bg-white px-4 text-sm text-black disabled:opacity-40" disabled={!online || busy || !custom.trim()}>
            {t("Install")}
          </button>
        </form>
      </div>
    </div>
  );
}
