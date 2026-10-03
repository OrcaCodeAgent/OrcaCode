import { CAPABILITIES } from "../lib/catalog";
import { useT } from "../lib/i18n";
import { useUi } from "../stores/ui";

export function PluginsView() {
  const enabled = useUi((state) => state.enabledPlugins);
  const toggle = useUi((state) => state.togglePlugin);
  const t = useT();
  return (
    <div className="scroll-thin flex-1 overflow-auto px-8 py-6">
      <div className="mx-auto max-w-xl">
        <h1 className="text-xl font-medium">{t("Capabilities")}</h1>
        <p className="mt-2 text-sm text-muted">{t("Computer, Documents, Web, Code, Automations. Enabled capabilities show up in the composer + menu and in @mentions.")}</p>
        <div className="mt-5 space-y-2">
          {CAPABILITIES.map((plugin) => {
            const on = enabled.includes(plugin.id);
            return (
              <button key={plugin.id} className="flex w-full items-center gap-3 rounded-xl border border-line bg-panel-2 px-3 py-3 text-left" onClick={() => toggle(plugin.id)}>
                <div className="min-w-0">
                  <div className="text-sm">{plugin.name}</div>
                  <div className="text-xs text-muted">{t(plugin.description)}</div>
                </div>
                <span className={`ml-auto h-5 w-9 shrink-0 rounded-full p-0.5 ${on ? "bg-white" : "bg-elev"}`} aria-hidden="true">
                  <span className={`block h-4 w-4 rounded-full ${on ? "ml-auto bg-black" : "bg-muted"}`} />
                </span>
              </button>
            );
          })}
        </div>
      </div>
    </div>
  );
}
