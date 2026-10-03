import { useEffect, useRef, useState, type ReactNode } from "react";
import { open } from "@tauri-apps/plugin-dialog";

import { api, explain } from "../lib/api";
import {
  CAPABILITIES,
  SLASH,
  approvedPlan,
  approvalLabel,
  buildInstructions,
  effortLabel,
  intentSuggestions,
  isPlainChat,
  previewEvidence,
  previewInstructions,
  runModeLabel,
  scopeInstructions,
  type Approval,
  type Effort,
  type RunMode,
} from "../lib/catalog";
import { useT } from "../lib/i18n";
import { allSkills, useUi } from "../stores/ui";
import { isRunning, useSession } from "../stores/session";
import { useSettings } from "../stores/settings";
import { IconPlus, IconSend, IconStop } from "./icons";

export function Composer({ centered = false }: { centered?: boolean }) {
  const [text, setText] = useState("");
  const [menu, setMenu] = useState<"plus" | "slash" | "skill" | "mention" | "model" | "mode" | "approval" | "env" | "goal" | null>(null);
  const [filter, setFilter] = useState("");
  const [goalDraft, setGoalDraft] = useState("");
  const lastPrompt = useRef("");
  const box = useRef<HTMLTextAreaElement>(null);
  const workspacePath = useSession((state) => state.workspacePath);
  const desktopPath = useSession((state) => state.desktopPath);
  const attachments = useSession((state) => state.attachments);
  const proposal = useSession((state) => state.proposal);
  const setAttachments = useSession((state) => state.setAttachments);
  const setProposal = useSession((state) => state.setProposal);
  const conversationId = useSession((state) => state.conversationId);
  const agentState = useSession((state) => state.agentState);
  const models = useSession((state) => state.models);
  const messages = useSession((state) => state.messages);
  const pushUser = useSession((state) => state.pushUser);
  const beginRun = useSession((state) => state.beginRun);
  const setConversations = useSession((state) => state.setConversations);
  const setBanner = useSession((state) => state.setBanner);
  const setAgentIdle = useSession((state) => state.applyEvent);
  const settings = useSettings((state) => state.settings);
  const patchSettings = useSettings((state) => state.patch);
  const effort = useUi((state) => state.effort);
  const approval = useUi((state) => state.approval);
  const environment = useUi((state) => state.environment);
  const runMode = useUi((state) => state.runMode);
  const personality = useUi((state) => state.personality);
  const language = useUi((state) => state.language);
  const t = useT();
  const skills = useUi((state) => state.skills);
  const enabledPlugins = useUi((state) => state.enabledPlugins);
  const goals = useUi((state) => state.goals);
  const patch = useUi((state) => state.patch);
  const setNotice = useUi((state) => state.setNotice);
  const setGoal = useUi((state) => state.setGoal);
  const moveGoal = useUi((state) => state.moveGoal);
  const automationWatch = useSession((state) => state.automationWatch);
  const running = isRunning(agentState) || Boolean(automationWatch);
  const goalKey = conversationId ?? "draft";
  const goal = goals[goalKey] ?? "";
  const skillList = allSkills(skills);

  useEffect(() => {
    const node = box.current;
    if (!node) return;
    node.style.height = "0px";
    node.style.height = `${Math.min(node.scrollHeight, 160)}px`;
  }, [text]);

  async function persist(partial: Partial<typeof settings>) {
    const next = { ...settings, ...partial };
    patchSettings(partial);
    try {
      await api.saveSettings(next);
    } catch (error) {
      setBanner(explain(error));
    }
  }

  async function ensureWorkspace() {
    if (environment === "worktree") {
      if (isPlainChat(workspacePath, desktopPath)) {
        setBanner(t("A worktree can only be created inside a project folder."));
        return null;
      }
      if (!workspacePath) return null;
      try {
        const created = await api.createWorktree(workspacePath);
        const remembered = await api.rememberWorkspace(created);
        useSession.getState().setWorkspace(remembered.path);
        useSession.getState().setWorkspaces(await api.listWorkspaces());
        patch({ environment: "local" });
        setNotice(t("Working in a worktree. {path}", { path: remembered.path }));
        return remembered.path;
      } catch (error) {
        setBanner(explain(error));
        return null;
      }
    }
    if (workspacePath) return workspacePath;
    if (!desktopPath) return null;
    try {
      const remembered = await api.rememberWorkspace(desktopPath);
      useSession.getState().setWorkspace(remembered.path);
      useSession.getState().setWorkspaces(await api.listWorkspaces());
      return remembered.path;
    } catch (error) {
      setBanner(explain(error));
      return null;
    }
  }

  const plainChat = isPlainChat(workspacePath, desktopPath);
  const folderLabel = workspacePath?.replace(/^\/Users\/[^/]+/, "~").split("/").filter(Boolean).at(-1) ?? "Folder";

  function instructionBlock(goalText: string) {
    return buildInstructions({
      text: goalText,
      personality,
      effort,
      goal,
      skills: skillList,
      enabledPlugins,
      language,
      scope: scopeInstructions(plainChat, folderLabel),
    });
  }

  async function launch(input: {
    goalText: string;
    mode: string;
    folder: string;
    instructions: string;
    recordUser: boolean;
    showUser: boolean;
  }) {
    if (input.showUser) pushUser(input.goalText);
    try {
      const started = await api.startTask({
        conversationId,
        goal: input.goalText,
        mode: input.mode,
        workspacePath: input.folder,
        approval: input.mode === "ask" || input.mode === "plan" ? "default" : approval,
        effort,
        instructions: input.instructions,
        recordUser: input.recordUser,
      });
      if (!conversationId) moveGoal("draft", started.conversationId);
      beginRun(started.conversationId, started.taskId);
      setConversations(await api.listConversations());
    } catch (error) {
      setBanner(explain(error));
      setAgentIdle({ kind: "state", state: "failed", detail: explain(error) });
      if (input.mode === "plan") setProposal(null);
    }
  }

  async function send(override?: string, modeOverride?: RunMode) {
    const goalText = (override ?? text).trim();
    if (!goalText || running) return;
    const folder = await ensureWorkspace();
    if (!folder) {
      setBanner(environment === "worktree" ? t("Choose a folder before creating a worktree.") : t("Could not open the Desktop as the workspace."));
      return;
    }
    if (!settings.model) {
      setBanner(t("Choose a model first."));
      return;
    }
    const mode = modeOverride ?? runMode;
    if (!override) {
      lastPrompt.current = text;
      setText("");
    }
    setMenu(null);
    setAttachments([]);
    setProposal(null);
    if (mode === "ask") {
      await launch({
        goalText,
        mode: "ask",
        folder,
        instructions: instructionBlock(goalText),
        recordUser: true,
        showUser: true,
      });
      return;
    }
    setProposal({ phase: "drafting", mode, goal: goalText, text: "", editing: false });
    await launch({
      goalText,
      mode: "plan",
      folder,
      instructions: `${previewInstructions(mode, goalText)}\n\n${instructionBlock(goalText)}`,
      recordUser: true,
      showUser: true,
    });
  }

  async function approveProposal() {
    if (!proposal || proposal.phase !== "ready" || running) return;
    const planText = proposal.text.trim();
    if (!planText) return;
    const folder = await ensureWorkspace();
    if (!folder) return;
    const mode = proposal.mode;
    const goalText = proposal.goal;
    setProposal(null);
    await launch({
      goalText,
      mode,
      folder,
      instructions: [approvedPlan(planText), previewEvidence(messages), instructionBlock(goalText)].filter(Boolean).join("\n\n"),
      recordUser: false,
      showUser: false,
    });
  }

  function onChange(value: string) {
    setText(value);
    if (value.startsWith("/")) {
      setMenu("slash");
      setFilter(value.slice(1).toLowerCase());
      return;
    }
    const token = value.split(/\s/).at(-1) ?? "";
    if (token.startsWith("$")) {
      setMenu("skill");
      setFilter(token.slice(1).toLowerCase());
      return;
    }
    if (token.startsWith("@")) {
      setMenu("mention");
      setFilter(token.slice(1).toLowerCase());
      return;
    }
    if (menu === "slash" || menu === "skill" || menu === "mention") setMenu(null);
  }

  function insertToken(token: string) {
    const parts = text.split(/\s/);
    parts[parts.length - 1] = token;
    setText(`${parts.join(" ")} `);
    setMenu(null);
    box.current?.focus();
  }

  function runSlash(id: string) {
    setText("");
    setMenu(null);
    if (id === "goal") {
      setGoalDraft(goal);
      setMenu("goal");
      return;
    }
    if (id === "model") {
      setMenu("model");
      return;
    }
    if (id === "fast") {
      patch({ effort: "low" });
      setNotice(t("Reasoning effort is now Low."));
      return;
    }
    if (id === "chat") {
      patch({ runMode: "ask" });
      void persist({ mode: "ask" });
      setNotice(t("Switched to Ask. It explains only and does not change files."));
      return;
    }
    if (id === "review") {
      void send(t("Review the uncommitted changes. Read the git diff and briefly list the risks and missing checks. Do not edit files."), "ask");
      return;
    }
    if (id === "status") {
      setNotice(
        t("Thread {thread} · {count} messages · {model} · context {context} · {effort} · {approval}", {
          thread: conversationId ?? t("New chat"),
          count: messages.length,
          model: settings.model || t("No model"),
          context: settings.contextLength,
          effort: t(effortLabel(effort)),
          approval: t(approvalLabel(approval)),
        }),
      );
      return;
    }
    const next = personality === "pragmatic" ? "friendly" : "pragmatic";
    patch({ personality: next });
    setNotice(next === "friendly" ? t("Personality is now Friendly.") : t("Personality is now Pragmatic."));
  }

  async function attachFile() {
    const selected = await open({ multiple: false, title: t("Attach a file") });
    if (typeof selected !== "string") return;
    setText((value) => `${value}${value.endsWith(" ") || value.length === 0 ? "" : " "}${selected} `);
    setMenu(null);
  }

  const slashItems = SLASH.filter((item) => item.id.includes(filter) || item.label.toLowerCase().includes(filter));
  const skillItems = skillList.filter((skill) => skill.name.includes(filter));
  const mentions = CAPABILITIES.filter((item) => enabledPlugins.includes(item.id) && item.name.toLowerCase().includes(filter)).map((item) => ({
    id: item.name,
    detail: item.description,
  }));
  const suggestions = intentSuggestions(attachments);

  return (
    <div className={`px-4 pb-4 ${centered ? "w-full max-w-[640px]" : "mx-auto w-full max-w-[720px]"}`}>
      {goal ? (
        <div className="mb-2 flex items-center gap-2 rounded-full border border-line bg-panel px-3 py-1 text-xs text-muted">
          <span className="truncate">Goal · {goal}</span>
          <button className="ml-auto" onClick={() => setGoal(goalKey, "")}>
            {t("Clear")}
          </button>
        </div>
      ) : null}
      <div className="relative">
        {menu === "plus" ? (
          <Menu className="bottom-[calc(100%+8px)] left-0 w-72">
            <MenuLabel>{t("Add")}</MenuLabel>
            <MenuButton onClick={() => void attachFile()}>{t("File")}</MenuButton>
            <MenuButton
              onClick={() => {
                setMenu("goal");
                setGoalDraft(goal);
              }}
            >
              Goal
              <span className="ml-auto text-xs text-muted">{t("Keep the goal")}</span>
            </MenuButton>
            <MenuLabel>{t("Capabilities")}</MenuLabel>
            {CAPABILITIES.map((capability) => (
              <MenuButton key={capability.id} onClick={() => insertToken(`@${capability.name}`)} disabled={!enabledPlugins.includes(capability.id)}>
                {capability.name}
                <span className="ml-auto text-xs text-muted">{enabledPlugins.includes(capability.id) ? t(capability.description) : t("Turn on in Capabilities")}</span>
              </MenuButton>
            ))}
          </Menu>
        ) : null}
        {menu === "slash" ? (
          <Menu className="bottom-[calc(100%+8px)] left-3 w-80">
            {slashItems.map((item) => (
              <MenuButton key={item.id} onClick={() => runSlash(item.id)}>
                /{item.id}
                <span className="ml-auto truncate pl-3 text-xs text-muted">{t(item.detail)}</span>
              </MenuButton>
            ))}
          </Menu>
        ) : null}
        {menu === "skill" ? (
          <Menu className="bottom-[calc(100%+8px)] left-3 w-80">
            {skillItems.map((skill) => (
              <MenuButton key={skill.id} onClick={() => insertToken(`$${skill.name}`)}>
                ${skill.name}
                <span className="ml-auto truncate pl-3 text-xs text-muted">{t(skill.description)}</span>
              </MenuButton>
            ))}
          </Menu>
        ) : null}
        {menu === "mention" ? (
          <Menu className="bottom-[calc(100%+8px)] left-3 w-80">
            {mentions.map((item) => (
              <MenuButton key={item.id} onClick={() => insertToken(`@${item.id}`)}>
                @{item.id}
                <span className="ml-auto truncate pl-3 text-xs text-muted">{t(item.detail)}</span>
              </MenuButton>
            ))}
          </Menu>
        ) : null}
        {menu === "model" ? (
          <Menu className="bottom-[calc(100%+8px)] left-12 w-64">
            <MenuLabel>{t("Model")}</MenuLabel>
            {(models.length > 0 ? models : [settings.model].filter(Boolean)).map((model) => (
              <MenuButton
                key={model}
                onClick={() => {
                  void persist({ model });
                  setMenu(null);
                }}
              >
                {model}
                {model === settings.model ? <span className="ml-auto text-xs">✓</span> : null}
              </MenuButton>
            ))}
            <MenuLabel>{t("Reasoning")}</MenuLabel>
            {(["low", "medium", "high", "xhigh"] as Effort[]).map((item) => (
              <MenuButton
                key={item}
                onClick={() => {
                  patch({ effort: item });
                  setMenu(null);
                }}
              >
                {t(effortLabel(item))}
                {effort === item ? <span className="ml-auto text-xs">✓</span> : null}
              </MenuButton>
            ))}
          </Menu>
        ) : null}
        {menu === "mode" ? (
          <Menu className="bottom-[calc(100%+8px)] left-36 w-56">
            {(["ask", "do", "mission"] as RunMode[]).map((item) => (
              <MenuButton
                key={item}
                onClick={() => {
                  patch({ runMode: item });
                  void persist({ mode: item });
                  setMenu(null);
                }}
              >
                {runModeLabel(item)}
                <span className="ml-auto text-xs text-muted">{item === "ask" ? t("Explain only") : item === "do" ? t("One job") : t("See it through")}</span>
              </MenuButton>
            ))}
          </Menu>
        ) : null}
        {menu === "approval" ? (
          <Menu className="bottom-[calc(100%+8px)] left-52 w-64">
            {(
              [
                ["default", "Ask before acting"],
                ["auto", "Auto-approve file edits"],
                ["full", "Auto-approve risky actions"],
              ] as const
            ).map(([id, detail]) => (
              <MenuButton
                key={id}
                onClick={() => {
                  patch({ approval: id as Approval });
                  setMenu(null);
                }}
              >
                {t(approvalLabel(id))}
                <span className="ml-auto text-xs text-muted">{t(detail)}</span>
              </MenuButton>
            ))}
          </Menu>
        ) : null}
        {menu === "env" ? (
          <Menu className="bottom-[calc(100%+8px)] right-12 w-64">
            <MenuButton
              onClick={() => {
                patch({ environment: "local" });
                setMenu(null);
              }}
            >
              Local
              <span className="ml-auto text-xs text-muted">{t("This folder")}</span>
            </MenuButton>
            <MenuButton
              disabled={plainChat}
              onClick={() => {
                if (plainChat) return;
                patch({ environment: "worktree" });
                setMenu(null);
              }}
            >
              Worktree
              <span className="ml-auto text-xs text-muted">{t("Separate Git checkout")}</span>
            </MenuButton>
            <MenuButton disabled>
              Cloud
              <span className="ml-auto text-xs text-muted">{t("Local model")}</span>
            </MenuButton>
          </Menu>
        ) : null}
        {menu === "goal" ? (
          <Menu className="bottom-[calc(100%+8px)] left-0 w-80 p-3">
            <div className="mb-2 text-xs text-muted">{t("Goal this thread keeps following")}</div>
            <textarea className="field h-20 text-sm" value={goalDraft} onChange={(event) => setGoalDraft(event.target.value)} />
            <div className="mt-2 flex justify-end gap-2">
              <button className="rounded-md px-2 py-1 text-xs text-muted" onClick={() => setMenu(null)}>
                {t("Cancel")}
              </button>
              <button
                className="rounded-full bg-white px-3 py-1 text-xs text-black"
                onClick={() => {
                  setGoal(goalKey, goalDraft.trim());
                  setMenu(null);
                }}
              >
                {t("Save")}
              </button>
            </div>
          </Menu>
        ) : null}
        {suggestions.length > 0 ? (
          <div className="mb-2 rounded-2xl border border-line bg-panel-2 px-3 py-3">
            <div className="text-sm">{t("Do this")}</div>
            <p className="mt-1 text-xs text-muted">{t("Look at {count} files and pick a job.", { count: attachments.length })}</p>
            <div className="mt-2 flex flex-wrap gap-2">
              {suggestions.map((item) => (
                <button
                  key={item.label}
                  className="rounded-full border border-line px-3 py-1 text-xs hover:bg-elev"
                  onClick={() => {
                    setAttachments([]);
                    void send(`${t(item.prompt)}\n${attachments.join("\n")}`);
                  }}
                >
                  {t(item.label)}
                </button>
              ))}
              <button className="rounded-full px-3 py-1 text-xs text-muted" onClick={() => setAttachments([])}>
                {t("Cancel")}
              </button>
            </div>
          </div>
        ) : null}
        {proposal?.phase === "drafting" ? (
          <div className="mb-2 flex items-center justify-between px-1 text-xs text-muted">
            <span>{t("Checking what to do")}</span>
            <button
              onClick={() => {
                setProposal(null);
                void api.cancelTask().catch((error) => setBanner(explain(error)));
              }}
            >
              {t("Cancel")}
            </button>
          </div>
        ) : null}
        {proposal?.phase === "ready" ? (
          <div className="mb-2 rounded-2xl border border-line bg-panel-2 px-4 py-3">
            {proposal.editing ? (
              <textarea
                className="field h-28 text-sm"
                value={proposal.text}
                onChange={(event) => setProposal({ ...proposal, text: event.target.value })}
              />
            ) : (
              <p className="whitespace-pre-wrap text-sm leading-6">{proposal.text}</p>
            )}
            <div className="mt-3 flex items-center gap-2 text-sm">
              <button className="rounded-full bg-white px-3 py-1.5 text-black" onClick={() => void approveProposal()}>
                {t("Run")}
              </button>
              <button
                className="rounded-full border border-line px-3 py-1.5"
                onClick={() => setProposal({ ...proposal, editing: !proposal.editing })}
              >
                {t("Edit plan")}
              </button>
              <button
                className="rounded-full px-3 py-1.5 text-muted"
                onClick={() => {
                  setProposal(null);
                  if (running) void api.cancelTask().catch((error) => setBanner(explain(error)));
                }}
              >
                {t("Cancel")}
              </button>
            </div>
          </div>
        ) : null}
        <div className="rounded-[22px] border border-line bg-panel-2 shadow-[0_0_0_1px_rgba(255,255,255,0.02)]">
          <textarea
            ref={box}
            className="max-h-40 min-h-[44px] w-full resize-none bg-transparent px-4 pt-3.5 text-[15px] outline-none placeholder:text-muted"
            placeholder={plainChat ? t("Say what to do. New files go on the Desktop.") : t("Say what to do in this folder.")}
            value={text}
            onChange={(event) => onChange(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "ArrowUp" && text.length === 0 && lastPrompt.current) {
                event.preventDefault();
                setText(lastPrompt.current);
              }
              if (event.key === "Escape") setMenu(null);
              if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing && menu !== "goal") {
                event.preventDefault();
                void send();
              }
            }}
          />
          <div className="flex flex-wrap items-center gap-1 px-2 pb-2">
            <IconButton title={t("Add")} onClick={() => setMenu(menu === "plus" ? null : "plus")}>
              <IconPlus />
            </IconButton>
            <Chip onClick={() => setMenu(menu === "model" ? null : "model")}>{settings.model ? shortModel(settings.model) : t("Model")} · {t(effortLabel(effort))}</Chip>
            <Chip onClick={() => setMenu(menu === "mode" ? null : "mode")}>{t(runModeLabel(runMode))}</Chip>
            {runMode !== "ask" ? <Chip onClick={() => setMenu(menu === "approval" ? null : "approval")}>{t(approvalLabel(approval))}</Chip> : null}
            <Chip onClick={() => setMenu(menu === "env" ? null : "env")}>{environment === "worktree" ? "Worktree" : "Local"}</Chip>
            <div className="ml-auto" />
            {running ? (
              <button
                className="flex h-8 w-8 items-center justify-center rounded-full bg-white text-black"
                title={t("Stop")}
                onClick={() => void api.cancelTask().catch((error) => setBanner(explain(error)))}
              >
                <IconStop />
              </button>
            ) : (
              <button
                className="flex h-8 w-8 items-center justify-center rounded-full bg-white text-black disabled:bg-elev disabled:text-muted"
                title={t("Send")}
                disabled={!text.trim()}
                onClick={() => void send()}
              >
                <IconSend />
              </button>
            )}
          </div>
        </div>
        {centered && messages.length === 0 && !proposal ? (
          <div className="mt-4 flex flex-wrap justify-center gap-2">
            {(plainChat ? CHAT_PROMPTS : FOLDER_PROMPTS).map((item) => (
              <button
                key={item.label}
                className="rounded-full border border-line px-3 py-1.5 text-xs text-muted hover:bg-elev hover:text-text"
                onClick={() => void send(t(item.prompt), item.mode)}
              >
                {t(item.label)}
              </button>
            ))}
          </div>
        ) : null}
      </div>
    </div>
  );
}

const CHAT_PROMPTS: { label: string; prompt: string; mode: RunMode }[] = [
  { label: "Tidy the Desktop", prompt: "Look at the Desktop and show a plan that groups files by type. Do not touch other folders.", mode: "do" },
  { label: "Make a note", prompt: "Create today's note on the Desktop. Do not overwrite existing files.", mode: "do" },
  { label: "Summarize the latest document", prompt: "Find the newest document on the Desktop and summarize it briefly.", mode: "ask" },
];

const FOLDER_PROMPTS: { label: string; prompt: string; mode: RunMode }[] = [
  { label: "Look through the folder", prompt: "Briefly explain this folder's structure and the important files. Do not change files.", mode: "ask" },
  { label: "Find what to change", prompt: "Find only the things in this folder that are worth fixing now. Show edits as a plan.", mode: "do" },
  { label: "Run tests", prompt: "Run this folder's build or tests and briefly list only the failures.", mode: "do" },
];

function shortModel(model: string): string {
  const name = model.split("/").at(-1) ?? model;
  return name.length > 22 ? `${name.slice(0, 20)}…` : name;
}

function Menu({ className, children }: { className: string; children: ReactNode }) {
  return <div className={`popover absolute z-10 py-1 ${className}`}>{children}</div>;
}

function MenuLabel({ children }: { children: ReactNode }) {
  return <div className="px-3 py-1 text-[11px] text-muted">{children}</div>;
}

function MenuButton({ children, onClick, disabled }: { children: ReactNode; onClick?: () => void; disabled?: boolean }) {
  return (
    <button className="flex w-full items-center gap-2 px-3 py-1.5 text-left text-sm hover:bg-elev disabled:text-muted" onClick={onClick} disabled={disabled}>
      {children}
    </button>
  );
}

function Chip({ children, onClick }: { children: ReactNode; onClick: () => void }) {
  return (
    <button className="max-w-full truncate rounded-full px-2 py-1 text-xs text-muted hover:bg-elev hover:text-text" onClick={onClick}>
      {children}
    </button>
  );
}

function IconButton({ children, onClick, title }: { children: ReactNode; onClick: () => void; title: string }) {
  return (
    <button className="flex h-8 w-8 items-center justify-center rounded-full text-muted hover:bg-elev hover:text-text" title={title} onClick={onClick}>
      {children}
    </button>
  );
}
