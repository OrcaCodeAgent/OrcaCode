import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";

import { ArchiveView } from "./components/ArchiveView";
import { AutomationsView } from "./components/AutomationsView";
import { CommandPalette } from "./components/CommandPalette";
import { Composer } from "./components/Composer";
import { FindBar, MessageList } from "./components/MessageList";
import { Inspector } from "./components/Inspector";
import { PermissionModal } from "./components/PermissionModal";
import { PluginsView } from "./components/PluginsView";
import { SettingsView } from "./components/SettingsView";
import { SetupScreen } from "./components/SetupScreen";
import { Sidebar } from "./components/Sidebar";
import { SkillsView } from "./components/SkillsView";
import { IconClose, IconSidebar } from "./components/icons";
import { Logo } from "./components/Logo";
import { api, explain } from "./lib/api";
import { findUpdate } from "./lib/updates";
import { automationDue, buildInstructions, isPlainChat, normalizeRunMode, samePath, scopeInstructions } from "./lib/catalog";
import { translate, useT } from "./lib/i18n";
import { selectWorkspace } from "./lib/workspace";
import { allSkills, useUi } from "./stores/ui";
import { isRunning, useSession } from "./stores/session";
import { useSettings } from "./stores/settings";
import type { AgentEvent } from "./types";

export function App() {
  const view = useSession((state) => state.view);
  const banner = useSession((state) => state.banner);
  const workspacePath = useSession((state) => state.workspacePath);
  const desktopPath = useSession((state) => state.desktopPath);
  const messages = useSession((state) => state.messages);
  const setWorkspace = useSession((state) => state.setWorkspace);
  const setOllama = useSession((state) => state.setOllama);
  const setConversations = useSession((state) => state.setConversations);
  const setWorkspaces = useSession((state) => state.setWorkspaces);
  const setBanner = useSession((state) => state.setBanner);
  const applyEvent = useSession((state) => state.applyEvent);
  const setView = useSession((state) => state.setView);
  const newTask = useSession((state) => state.newTask);
  const setInspector = useSession((state) => state.setInspector);
  const setSettings = useSettings((state) => state.setSettings);
  const sidebarOpen = useUi((state) => state.sidebarOpen);
  const taskOpen = useUi((state) => state.taskOpen);
  const fontScale = useUi((state) => state.fontScale);
  const notice = useUi((state) => state.notice);
  const patchUi = useUi((state) => state.patch);
  const setPalette = useUi((state) => state.setPalette);
  const setFind = useUi((state) => state.setFind);
  const setNotice = useUi((state) => state.setNotice);
  const language = useUi((state) => state.language);
  const t = useT();
  const [boot, setBoot] = useState<"pending" | "ready" | "error">("pending");
  const [bootError, setBootError] = useState<string | null>(null);
  const [bootVisible, setBootVisible] = useState(false);

  useEffect(() => {
    document.documentElement.lang = language;
  }, [language]);

  useEffect(() => {
    if (boot === "ready") {
      setBootVisible(false);
      return;
    }
    if (boot === "error") {
      setBootVisible(true);
      return;
    }
    const timer = window.setTimeout(() => setBootVisible(true), 400);
    return () => window.clearTimeout(timer);
  }, [boot]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void (async () => {
      try {
        const payload = await api.getSettings();
        setSettings(payload.settings, payload.defaultPrompt);
        if (!useUi.getState().modeSynced) {
          useUi.getState().markModeSynced(normalizeRunMode(payload.settings.mode));
        }
        const [home, desktop] = await Promise.all([api.computerHome(), api.computerDesktop()]);
        useSession.getState().setComputerHome(home);
        useSession.getState().setDesktopPath(desktop);
        const saved = payload.settings.workspacePath;
        setWorkspace(saved && !samePath(saved, desktop) ? saved : desktop);
        const [conversations, workspaces] = await Promise.all([api.listConversations(), api.listWorkspaces()]);
        setConversations(conversations);
        setWorkspaces(workspaces);
        const ready = await api.bootstrapRuntime();
        const current = useSettings.getState().settings;
        if (ready.model && current.model !== ready.model) {
          setSettings({ ...current, model: ready.model });
        }
        setOllama(ready.online, "Connected to Ollama.", ready.models);
        setBoot("ready");
      } catch (error) {
        const message = explain(error);
        if (message.includes("invoke")) {
          setBoot("ready");
          return;
        }
        setBootError(message);
        setBoot("error");
        setBanner(message);
      }
    })();
    void listen<AgentEvent>("agent-event", (event) => applyEvent(event.payload)).then((stop) => {
      unlisten = stop;
    });
    return () => unlisten?.();
  }, [applyEvent, setBanner, setConversations, setOllama, setSettings, setWorkspace, setWorkspaces]);

  useEffect(() => {
    let stop = () => {};
    try {
      void getCurrentWebview()
        .onDragDropEvent((event) => {
          if (event.payload.type === "drop") {
            useSession.getState().setAttachments(event.payload.paths);
          }
        })
        .then((unlisten) => {
          stop = unlisten;
        })
        .catch(() => undefined);
    } catch {
      return;
    }
    return () => stop();
  }, []);

  useEffect(() => {
    if (boot !== "ready") return;
    let cancelled = false;
    void findUpdate()
      .then(async (update) => {
        if (!update || cancelled) {
          await update?.close();
          return;
        }
        const version = update.version;
        await update.close();
        if (!cancelled) setNotice(translate(useUi.getState().language, "Update {version} is available. You can install it in Settings.", { version }));
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [boot, setNotice]);

  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      const meta = event.metaKey || event.ctrlKey;
      if (meta && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setPalette(!useUi.getState().paletteOpen);
      } else if (meta && event.key.toLowerCase() === "b" && !event.shiftKey) {
        event.preventDefault();
        patchUi({ sidebarOpen: !useUi.getState().sidebarOpen });
      } else if (meta && event.key === ",") {
        event.preventDefault();
        setView(useSession.getState().view === "settings" ? "chat" : "settings");
      } else if (meta && event.key.toLowerCase() === "n" && !event.shiftKey) {
        event.preventDefault();
        newTask();
        setView("chat");
      } else if (meta && event.key.toLowerCase() === "o" && !event.shiftKey) {
        event.preventDefault();
        void selectWorkspace().then((path) => {
          if (path) newTask(path);
        });
      } else if (meta && event.key.toLowerCase() === "j") {
        event.preventDefault();
        patchUi({ taskOpen: !useUi.getState().taskOpen });
      } else if (meta && event.key.toLowerCase() === "f") {
        event.preventDefault();
        setFind(true);
      } else if (meta && event.key.toLowerCase() === "g") {
        event.preventDefault();
        const ui = useUi.getState();
        ui.setFindIndex(ui.findIndex + (event.shiftKey ? -1 : 1));
      } else if (event.ctrlKey && event.key === "`") {
        event.preventDefault();
        patchUi({ taskOpen: true });
        setInspector("terminal");
      } else if (meta && (event.key === "=" || event.key === "+")) {
        event.preventDefault();
        patchUi({ fontScale: Math.min(1.4, useUi.getState().fontScale + 0.1) });
      } else if (meta && event.key === "-") {
        event.preventDefault();
        patchUi({ fontScale: Math.max(0.8, useUi.getState().fontScale - 0.1) });
      } else if (meta && event.key === "0") {
        event.preventDefault();
        patchUi({ fontScale: 1 });
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [newTask, patchUi, setFind, setInspector, setPalette, setView]);

  useEffect(() => {
    const timer = window.setInterval(() => {
      const ui = useUi.getState();
      const session = useSession.getState();
      const settings = useSettings.getState().settings;
      if (isRunning(session.agentState) || session.automationWatch || session.proposal || !settings.model) return;
      const now = Date.now();
      const due = ui.automations.find((item) => automationDue(item, now));
      if (!due) return;
      const background = session.messages.length > 0 || Boolean(session.conversationId);
      session.setAutomationWatch({ id: due.id, background });
      void (async () => {
        try {
          const started = await api.startTask({
            conversationId: null,
            goal: due.prompt,
            mode: "mission",
            workspacePath: due.workspacePath,
            approval: ui.approval,
            effort: ui.effort,
            instructions: buildInstructions({
              text: due.prompt,
              personality: ui.personality,
              effort: ui.effort,
              goal: "",
              skills: allSkills(ui.skills),
              enabledPlugins: ui.enabledPlugins,
              language: ui.language,
              scope: scopeInstructions(
                isPlainChat(due.workspacePath, session.desktopPath),
                due.workspacePath.split("/").filter(Boolean).at(-1) ?? "Folder",
              ),
            }),
          });
          ui.setNotice(translate(ui.language, "Automation started · {name}", { name: due.name }));
          if (!background) {
            session.setWorkspace(due.workspacePath);
            session.pushUser(due.prompt);
            session.beginRun(started.conversationId, started.taskId);
            session.setConversations(await api.listConversations());
            session.setView("chat");
          }
        } catch (error) {
          session.setAutomationWatch(null);
          ui.finishAutomation(due.id, false);
          session.setBanner(explain(error));
        }
      })();
    }, 30_000);
    return () => window.clearInterval(timer);
  }, []);

  const automations = useUi((state) => state.automations);
  useEffect(() => {
    void api.setRunsInBackground(automations.some((item) => item.enabled)).catch(() => undefined);
  }, [automations]);

  const empty = view === "chat" && messages.length === 0;
  const plainChat = isPlainChat(workspacePath, desktopPath);
  const rawFolder = headerTitle(workspacePath, desktopPath);
  const folderName = rawFolder === "Folder" || rawFolder === "Chat" ? t(rawFolder) : rawFolder;

  return (
    <div className="relative flex h-full bg-ink text-text" style={{ fontSize: `${14 * fontScale}px` }}>
      <Sidebar />
      <main className="flex min-w-0 flex-1 flex-col">
        <header className="flex h-11 items-center gap-2 px-4">
          {sidebarOpen ? null : (
            <button className="rounded-md p-1.5 text-muted hover:bg-elev" onClick={() => patchUi({ sidebarOpen: true })}>
              <IconSidebar />
            </button>
          )}
          <div className="min-w-0 flex-1 truncate text-[13px] text-muted">
            {plainChat ? t("Chat · Desktop") : folderName}
          </div>
          {view === "chat" && !plainChat ? (
            <button className="rounded-md px-2 py-1 text-xs text-muted hover:bg-elev hover:text-text" onClick={() => newTask()}>
              {t("Back to chat")}
            </button>
          ) : null}
          {view === "chat" ? (
            <button className="rounded-md px-2 py-1 text-xs text-muted hover:bg-elev hover:text-text" onClick={() => patchUi({ taskOpen: !taskOpen })}>
              {taskOpen ? t("Close panel") : t("Panel")}
            </button>
          ) : (
            <button className="flex h-8 w-8 items-center justify-center rounded-md text-muted hover:bg-elev hover:text-text" title={t("Close")} onClick={() => setView("chat")}>
              <IconClose />
            </button>
          )}
        </header>
        {banner ? <div className="mx-4 mb-2 rounded-xl border border-danger/40 bg-danger/10 px-3 py-2 text-sm text-danger">{banner}</div> : null}
        {notice ? (
          <button className="mx-4 mb-2 rounded-xl border border-line bg-panel-2 px-3 py-2 text-left text-sm text-muted" onClick={() => setNotice(null)}>
            {notice}
          </button>
        ) : null}
        {view === "settings" ? <SettingsView /> : null}
        {view === "skills" ? <SkillsView /> : null}
        {view === "automations" ? <AutomationsView /> : null}
        {view === "plugins" ? <PluginsView /> : null}
        {view === "archive" ? <ArchiveView /> : null}
        {view === "chat" ? (
          empty ? (
            <div className="flex flex-1 flex-col items-center justify-center px-6 pb-10">
              <Logo className="mb-5 h-14 w-14" />
              <h1 className="text-center text-[32px] font-medium tracking-[-0.03em]">
                {plainChat ? t("What should we look at?") : t("What should we do in {folder}?", { folder: folderName })}
              </h1>
              <p className="mb-6 mt-2 text-center text-sm text-muted">
                {plainChat ? t("New files go on the Desktop.") : t("Files change only inside this folder.")}
              </p>
              <Composer centered />
            </div>
          ) : (
            <>
              <FindBar />
              <MessageList />
              <Composer />
            </>
          )
        ) : null}
      </main>
      {view === "chat" && taskOpen ? <Inspector /> : null}
      <PermissionModal />
      <CommandPalette />
      {bootVisible ? (
        <SetupScreen
          error={bootError}
          onRetry={() => {
            setBoot("pending");
            setBootError(null);
            setBanner(null);
            void api
              .bootstrapRuntime()
              .then((ready) => {
                const current = useSettings.getState().settings;
                if (ready.model && current.model !== ready.model) {
                  setSettings({ ...current, model: ready.model });
                }
                setOllama(ready.online, "Connected to Ollama.", ready.models);
                setBoot("ready");
              })
              .catch((error) => {
                const message = explain(error);
                setBootError(message);
                setBoot("error");
                setBanner(message);
              });
          }}
        />
      ) : null}
    </div>
  );
}

function headerTitle(path: string | null, desktop: string | null): string {
  if (isPlainChat(path, desktop)) return "Chat";
  const shortened = (path ?? "").replace(/^\/Users\/[^/]+/, "~");
  return shortened.split("/").filter(Boolean).at(-1) ?? "Folder";
}
