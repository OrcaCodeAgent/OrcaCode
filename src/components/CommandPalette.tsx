import { useEffect, useMemo, useState } from "react";

import { useT } from "../lib/i18n";
import { useSession } from "../stores/session";
import { useUi } from "../stores/ui";
import { selectWorkspace } from "../lib/workspace";

export function CommandPalette() {
  const open = useUi((state) => state.paletteOpen);
  const setPalette = useUi((state) => state.setPalette);
  const patch = useUi((state) => state.patch);
  const setView = useSession((state) => state.setView);
  const newTask = useSession((state) => state.newTask);
  const [query, setQuery] = useState("");
  const t = useT();
  const [index, setIndex] = useState(0);

  const actions = useMemo(
    () => [
      { id: "new", label: "New chat", hint: "⌘N", run: () => { newTask(); setView("chat"); } },
      { id: "folder", label: "Open folder", hint: "⌘O", run: () => void selectWorkspace().then((path) => { if (path) newTask(path); }) },
      { id: "sidebar", label: "Toggle sidebar", hint: "⌘B", run: () => patch({ sidebarOpen: !useUi.getState().sidebarOpen }) },
      { id: "task", label: "Toggle task panel", hint: "⌘J", run: () => patch({ taskOpen: !useUi.getState().taskOpen }) },
      { id: "find", label: "Find in chat", hint: "⌘F", run: () => useUi.getState().setFind(true) },
      { id: "ask", label: "Ask", run: () => patch({ runMode: "ask" }) },
      { id: "do", label: "Do", run: () => patch({ runMode: "do" }) },
      { id: "mission", label: "Mission", run: () => patch({ runMode: "mission" }) },
      { id: "skills", label: "Skills", run: () => setView("skills") },
      { id: "auto", label: "Automations", run: () => setView("automations") },
      { id: "plugins", label: "Capabilities", run: () => setView("plugins") },
      { id: "settings", label: "Settings", hint: "⌘,", run: () => setView("settings") },
    ],
    [newTask, patch, setView],
  );

  const visible = actions.filter((action) => {
    const needle = query.trim().toLowerCase();
    return action.label.toLowerCase().includes(needle) || t(action.label).toLowerCase().includes(needle);
  });

  useEffect(() => {
    if (!open) {
      setQuery("");
      setIndex(0);
    }
  }, [open]);

  if (!open) return null;

  function choose(position = index) {
    const action = visible[position];
    if (!action) return;
    action.run();
    setPalette(false);
  }

  return (
    <div className="fixed inset-0 z-30 flex items-start justify-center bg-black/50 px-4 pt-[18vh]" onMouseDown={() => setPalette(false)}>
      <div className="popover w-full max-w-lg overflow-hidden" onMouseDown={(event) => event.stopPropagation()}>
        <input
          autoFocus
          className="w-full bg-transparent px-4 py-3 text-sm outline-none"
          placeholder={t("Search commands")}
          value={query}
          onChange={(event) => {
            setQuery(event.target.value);
            setIndex(0);
          }}
          onKeyDown={(event) => {
            if (event.key === "Escape") setPalette(false);
            if (event.key === "ArrowDown") {
              event.preventDefault();
              setIndex((value) => Math.min(value + 1, Math.max(visible.length - 1, 0)));
            }
            if (event.key === "ArrowUp") {
              event.preventDefault();
              setIndex((value) => Math.max(value - 1, 0));
            }
            if (event.key === "Enter") {
              event.preventDefault();
              choose();
            }
          }}
        />
        <div className="max-h-72 overflow-auto border-t border-line py-1">
          {visible.length === 0 ? <div className="px-4 py-3 text-sm text-muted">{t("No matching commands.")}</div> : null}
          {visible.map((action, position) => (
            <button
              key={action.id}
              className={`flex w-full items-center px-4 py-2 text-left text-sm ${position === index ? "bg-elev" : ""}`}
              onMouseEnter={() => setIndex(position)}
              onClick={() => choose(position)}
            >
              <span>{t(action.label)}</span>
              {action.hint ? <span className="ml-auto text-xs text-muted">{action.hint}</span> : null}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
