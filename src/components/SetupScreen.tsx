import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";

import { useT } from "../lib/i18n";
import { Logo } from "./Logo";

interface Progress {
  status: string;
  completed: number;
  total: number;
}

export function SetupScreen({ error, onRetry }: { error: string | null; onRetry: () => void }) {
  const t = useT();
  const [line, setLine] = useState("Preparing Ollama and the default model");
  const [ratio, setRatio] = useState<number | null>(null);

  useEffect(() => {
    let stop = () => {};
    void listen<Progress>("ollama-progress", (event) => {
      setLine(event.payload.status || "Preparing");
      if (event.payload.total > 0) {
        setRatio(Math.max(0, Math.min(1, event.payload.completed / event.payload.total)));
      }
    }).then((unlisten) => {
      stop = unlisten;
    });
    return () => stop();
  }, []);

  return (
    <div className="absolute inset-0 z-40 flex items-center justify-center bg-ink/95 px-6">
      <div className="w-full max-w-md rounded-2xl border border-line bg-panel-2 px-6 py-7">
        <Logo className="mb-4 h-12 w-12" />
        <h1 className="text-lg font-medium">{t("Prepare Orca")}</h1>
        <p className="mt-2 text-sm text-muted">{t("This prepares Ollama and the default model qwen2.5-coder:7b on this device. The first download takes a while.")}</p>
        <p className="mt-4 text-sm">{error ?? t(line)}</p>
        {ratio !== null && !error ? (
          <div className="mt-3 h-1.5 overflow-hidden rounded-full bg-elev">
            <div className="h-full bg-text" style={{ width: `${Math.round(ratio * 100)}%` }} />
          </div>
        ) : null}
        {error ? (
          <button className="mt-5 rounded-lg bg-text px-3 py-1.5 text-sm text-ink" onClick={onRetry}>
            {t("Try again")}
          </button>
        ) : null}
      </div>
    </div>
  );
}
