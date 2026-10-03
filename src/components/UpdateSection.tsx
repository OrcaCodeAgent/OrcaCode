import { useEffect, useRef, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { relaunch } from "@tauri-apps/plugin-process";
import type { Update } from "@tauri-apps/plugin-updater";

import { useT } from "../lib/i18n";
import { findUpdate, updateError } from "../lib/updates";
import { useUi } from "../stores/ui";

export function UpdateSection() {
  const setNotice = useUi((state) => state.setNotice);
  const t = useT();
  const pending = useRef<Update | null>(null);
  const [version, setVersion] = useState("0.1.0");
  const [available, setAvailable] = useState<string | null>(null);
  const [phase, setPhase] = useState<"idle" | "checking" | "installing">("idle");

  useEffect(() => {
    void getVersion().then(setVersion).catch(() => undefined);
    return () => {
      void pending.current?.close();
      pending.current = null;
    };
  }, []);

  async function look() {
    setPhase("checking");
    try {
      await pending.current?.close();
      pending.current = null;
      const found = await findUpdate();
      pending.current = found;
      setAvailable(found?.version ?? null);
      setNotice(found ? t("Update {version} is available.", { version: found.version }) : t("Already up to date."));
    } catch (error) {
      const message = updateError(error);
      if (!message.includes("invoke")) setNotice(message);
    } finally {
      setPhase("idle");
    }
  }

  async function install() {
    const found = pending.current;
    if (!found) return;
    setPhase("installing");
    try {
      await found.downloadAndInstall();
      await relaunch();
    } catch (error) {
      const message = updateError(error);
      if (!message.includes("invoke")) setNotice(message);
      setPhase("idle");
    }
  }

  return (
    <div className="rounded-xl border border-line bg-panel-2 px-4 py-3">
      <div className="flex items-center gap-3">
        <div className="min-w-0">
          <div className="text-sm">Orca {version}</div>
          <div className="mt-0.5 text-xs text-muted">{available ? t("Version {version} is ready to install", { version: available }) : t("Checks GitHub Releases for a new version.")}</div>
        </div>
        <button className="ml-auto shrink-0 rounded-md border border-line bg-elev px-3 py-1.5 text-sm disabled:opacity-50" disabled={phase !== "idle"} onClick={() => void look()}>
          {phase === "checking" ? t("Checking...") : t("Check for updates")}
        </button>
      </div>
      {available ? (
        <button className="mt-3 rounded-md bg-text px-3 py-1.5 text-sm text-ink disabled:opacity-50" disabled={phase === "installing"} onClick={() => void install()}>
          {phase === "installing" ? t("Installing") : t("Install and restart")}
        </button>
      ) : null}
    </div>
  );
}
