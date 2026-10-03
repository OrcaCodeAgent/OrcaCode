import { useEffect } from "react";

import { api, explain } from "../lib/api";
import { useT } from "../lib/i18n";
import { useSession } from "../stores/session";

export function PermissionModal() {
  const permission = useSession((state) => state.permission);
  const setBanner = useSession((state) => state.setBanner);
  const t = useT();

  useEffect(() => {
    if (!permission) return;
    function onKey(event: KeyboardEvent) {
      if (event.key === "Escape") {
        event.preventDefault();
        void answer("deny");
      }
      if (event.key === "Enter" && !event.shiftKey && !(event.target instanceof HTMLTextAreaElement)) {
        event.preventDefault();
        void answer("once");
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [permission]);

  if (!permission) return null;
  const args = JSON.stringify(permission.arguments, null, 2);

  async function answer(decision: "allow" | "once" | "deny") {
    try {
      await api.respondPermission(permission!.id, decision);
    } catch (error) {
      setBanner(explain(error));
    }
  }

  return (
    <div className="pointer-events-none fixed inset-x-0 bottom-28 z-20 flex justify-center px-4">
      <div className="pointer-events-auto w-full max-w-xl rounded-2xl border border-line bg-panel-2 p-4 shadow-[0_24px_80px_rgba(0,0,0,0.55)]">
        <div className="text-xs text-warn">{t("Approval required")}</div>
        <div className="mt-1 font-medium">{t(permission.summary)}</div>
        <div className="mt-2 truncate font-mono text-xs text-muted">{permission.tool}</div>
        <pre className="scroll-thin mt-2 max-h-28 overflow-auto text-xs text-muted">{args}</pre>
        <div className="mt-3 flex items-center justify-end gap-2 text-sm">
          <span className="mr-auto text-xs text-muted">{t("{risk} · Enter once · Esc deny", { risk: permission.risk === "dangerous" ? t("Risk") : t("Caution") })}</span>
          <button className="rounded-full px-3 py-1.5 text-muted" onClick={() => void answer("deny")}>
            {t("Deny")}
          </button>
          <button className="rounded-full border border-line px-3 py-1.5" onClick={() => void answer("once")}>
            {t("Once")}
          </button>
          <button className="rounded-full bg-white px-3 py-1.5 text-black" onClick={() => void answer("allow")}>
            {t("Allow")}
          </button>
        </div>
      </div>
    </div>
  );
}
