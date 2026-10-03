import { useEffect, useMemo, useRef, useState } from "react";

import { api, explain, undoSummary } from "../lib/api";
import { useT } from "../lib/i18n";
import { useSession } from "../stores/session";
import { useUi } from "../stores/ui";
import type { ChatEntry, FileChange, PlanStep } from "../types";
import { DiffView } from "./DiffView";
import { MarkdownMessage } from "./MarkdownMessage";

export function MessageList() {
  const messages = useSession((state) => state.messages);
  const agentState = useSession((state) => state.agentState);
  const fileChanges = useSession((state) => state.fileChanges);
  const plan = useSession((state) => state.plan);
  const taskId = useSession((state) => state.taskId);
  const proposal = useSession((state) => state.proposal);
  const findQuery = useUi((state) => state.findQuery);
  const findOpen = useUi((state) => state.findOpen);
  const findIndex = useUi((state) => state.findIndex);
  const bottom = useRef<HTMLDivElement>(null);
  const query = findOpen ? findQuery.trim() : "";
  const hits = useMemo(() => matchingIds(messages, query), [messages, query]);
  const blocks = useMemo(() => groupMessages(messages), [messages]);
  const running = agentState === "thinking" || agentState === "executing_tool" || agentState === "verifying" || agentState === "waiting_permission";
  const t = useT();

  useEffect(() => {
    if (query) return;
    bottom.current?.scrollIntoView({ block: "end" });
  }, [messages, agentState, query]);

  useEffect(() => {
    const id = hits[findIndex];
    if (!id) return;
    document.getElementById(`msg-${id}`)?.scrollIntoView({ block: "center" });
  }, [findIndex, hits]);

  return (
    <div className="scroll-thin flex-1 overflow-auto px-6 py-6">
      <div className="mx-auto flex max-w-[720px] flex-col gap-5">
        {blocks.map((block, index) =>
          block.kind === "entry" ? (
            <div id={`msg-${block.entry.id}`} key={block.entry.id}>
              <Entry entry={block.entry} query={query} active={hits[findIndex] === block.entry.id} />
            </div>
          ) : (
            <ProgressCard
              key={block.id}
              tools={block.tools}
              changes={fileChanges}
              query={query}
              plan={index === blocks.length - 1 ? plan : []}
              taskId={index === blocks.length - 1 ? taskId : null}
              running={running && index === blocks.length - 1}
              drafting={proposal?.phase === "drafting" && index === blocks.length - 1}
              planned={proposal?.phase === "ready" && index === blocks.length - 1}
            />
          ),
        )}
        {running && !messages.some((entry) => (entry.kind === "assistant" && entry.streaming) || entry.kind === "tool") ? (
          <div className="text-xs text-muted">{proposal?.phase === "drafting" ? t("Checking") : t("Working...")}</div>
        ) : null}
        <div ref={bottom} />
      </div>
    </div>
  );
}

function groupMessages(messages: ChatEntry[]): Array<{ kind: "entry"; entry: ChatEntry } | { kind: "progress"; id: string; tools: Extract<ChatEntry, { kind: "tool" }>[] }> {
  const blocks: Array<{ kind: "entry"; entry: ChatEntry } | { kind: "progress"; id: string; tools: Extract<ChatEntry, { kind: "tool" }>[] }> = [];
  for (const entry of messages) {
    if (entry.kind !== "tool") {
      blocks.push({ kind: "entry", entry });
      continue;
    }
    const last = blocks[blocks.length - 1];
    if (last?.kind === "progress") last.tools.push(entry);
    else blocks.push({ kind: "progress", id: entry.id, tools: [entry] });
  }
  return blocks;
}

function ProgressCard({
  tools,
  changes,
  query,
  plan,
  taskId,
  running,
  drafting,
  planned,
}: {
  tools: Extract<ChatEntry, { kind: "tool" }>[];
  changes: FileChange[];
  query: string;
  plan: PlanStep[];
  taskId: string | null;
  running: boolean;
  drafting: boolean;
  planned: boolean;
}) {
  const [details, setDetails] = useState<"commands" | "files" | "logs" | null>(null);
  const setInspector = useSession((state) => state.setInspector);
  const setBanner = useSession((state) => state.setBanner);
  const patch = useUi((state) => state.patch);
  const setNotice = useUi((state) => state.setNotice);
  const t = useT();
  const title = drafting ? t("Checking") : planned ? t("Plan") : running ? t("Working...") : t("Done");
  const lines = plan.length > 0 ? plan.map((step) => ({ id: step.id, mark: step.status === "completed" ? "✓" : step.status === "failed" ? "!" : "·", text: step.title })) : tools.filter((tool) => tool.name !== "update_plan").map((tool) => ({
    id: tool.id,
    mark: tool.status === "ok" ? "✓" : tool.status === "running" ? "·" : "!",
    text: checklistText(tool, t),
  }));
  const commands = tools.filter((tool) => tool.name === "terminal_execute" || tool.name === "process_start");
  const changed = changes.filter((change) => change.diff);

  async function undo() {
    if (!taskId) return;
    try {
      const restored = await api.undoTask(taskId);
      setNotice(undoSummary(restored));
    } catch (error) {
      setBanner(explain(error));
    }
  }

  return (
    <div className="rounded-2xl border border-line bg-panel-2 px-4 py-3 text-sm">
      <div className="text-muted">{title}</div>
      <div className="mt-2 space-y-1">
        {lines.map((line) => (
          <div id={`msg-${line.id}`} key={line.id} className="flex gap-2">
            <span className={line.mark === "!" ? "text-danger" : "text-ok"}>{line.mark}</span>
            <span>{line.text}</span>
          </div>
        ))}
      </div>
      <div className="mt-3 flex flex-wrap gap-2 text-xs">
        <button
          className="rounded-full bg-white px-3 py-1 text-black"
          onClick={() => {
            patch({ taskOpen: true });
            setInspector(changed.length > 0 ? "files" : "plan");
          }}
        >
          {t("View result")}
        </button>
        <button className="rounded-full border border-line px-3 py-1 disabled:text-muted" disabled={!taskId || running} onClick={() => void undo()}>
          {t("Undo")}
        </button>
        <button className={`rounded-full px-3 py-1 ${details === "commands" ? "bg-elev" : "text-muted"}`} onClick={() => setDetails(details === "commands" ? null : "commands")}>
          {t("Commands")}
        </button>
        <button className={`rounded-full px-3 py-1 ${details === "files" ? "bg-elev" : "text-muted"}`} onClick={() => setDetails(details === "files" ? null : "files")}>
          {t("File changes")}
        </button>
        <button className={`rounded-full px-3 py-1 ${details === "logs" ? "bg-elev" : "text-muted"}`} onClick={() => setDetails(details === "logs" ? null : "logs")}>
          {t("Logs")}
        </button>
      </div>
      {details === "commands" ? (
        <div className="mt-3 space-y-2">
          {commands.length === 0 ? <p className="text-xs text-muted">{t("No commands ran.")}</p> : null}
          {commands.map((tool) => (
            <pre key={tool.id} className="scroll-thin max-h-40 overflow-auto text-xs text-muted">
              <Highlight text={`${previewArgs(tool.arguments)}\n${tool.output}`} query={query} />
            </pre>
          ))}
        </div>
      ) : null}
      {details === "files" ? (
        <div className="mt-3 space-y-2">
          {changed.length === 0 ? <p className="text-xs text-muted">{t("No files were changed.")}</p> : null}
          {changed.map((change) => (
            <div key={change.path}>
              <div className="mb-1 truncate text-xs text-muted">{change.path}</div>
              <DiffView diff={change.diff} />
            </div>
          ))}
        </div>
      ) : null}
      {details === "logs" ? (
        <div className="mt-3 space-y-2">
          {tools.map((tool) => (
            <pre key={tool.id} className="scroll-thin max-h-40 overflow-auto text-xs text-muted">
              <Highlight text={`${t(toolLabel(tool.name))}\n${tool.output || t("No output.")}`} query={query} />
            </pre>
          ))}
        </div>
      ) : null}
    </div>
  );
}

function checklistText(entry: Extract<ChatEntry, { kind: "tool" }>, t: (text: string, vars?: Record<string, string | number>) => string): string {
  const summary = entry.output.match(/summary:\s*files=(\d+)\s+dirs=(\d+)/);
  if (entry.name === "list_directory" && summary) {
    return t("Folder listing · {files} files, {dirs} folders", { files: summary[1], dirs: summary[2] });
  }
  const target = previewArgs(entry.arguments);
  const label = t(toolLabel(entry.name));
  return target ? `${label} · ${target}` : label;
}

function matchingIds(messages: ChatEntry[], query: string): string[] {
  if (!query) return [];
  const needle = query.toLowerCase();
  return messages.filter((entry) => entryText(entry).toLowerCase().includes(needle)).map((entry) => entry.id);
}

function entryText(entry: ChatEntry): string {
  if (entry.kind === "tool") return `${entry.name} ${entry.output} ${previewArgs(entry.arguments)}`;
  return entry.content;
}

function Entry({ entry, query, active }: { entry: ChatEntry; query: string; active: boolean }) {
  const ring = active ? "rounded-xl ring-1 ring-[#e3b341]" : "";
  if (entry.kind === "user") {
    return (
      <div className={`ml-auto max-w-[80%] rounded-2xl bg-elev px-4 py-2.5 text-[15px] leading-6 ${ring}`}>
        <Highlight text={entry.content} query={query} />
      </div>
    );
  }
  if (entry.kind === "assistant") {
    return (
      <div className={ring}>
        <MarkdownMessage text={entry.content} query={query} />
        {entry.streaming ? <span className="ml-1 inline-block h-3 w-1.5 bg-white align-middle" /> : null}
      </div>
    );
  }
  if (entry.kind === "note") return <div className="text-xs text-warn">{entry.content}</div>;
  return null;
}

function Highlight({ text, query }: { text: string; query: string }) {
  if (!query) return <>{text}</>;
  const expression = new RegExp(`(${escapeRegExp(query)})`, "ig");
  const parts = text.split(expression);
  return (
    <>
      {parts.map((part, index) =>
        part.toLowerCase() === query.toLowerCase() ? (
          <mark key={`${part}-${index}`} className="find-hit">
            {part}
          </mark>
        ) : (
          <span key={`${part}-${index}`}>{part}</span>
        ),
      )}
    </>
  );
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function toolLabel(name: string): string {
  const labels: Record<string, string> = {
    terminal_execute: "Run command",
    read_file: "Read file",
    edit_file: "Edit file",
    write_file: "Create file",
    list_directory: "List folder",
    search_text: "Search text",
    search_files: "Search files",
    git_status: "Git status",
    git_diff: "Git diff",
    git_commit: "Commit",
    process_start: "Start process",
    process_stop: "Stop process",
    update_plan: "Update plan",
    screenshot: "Screenshot",
    open_url: "Browser",
    open_application: "Open app",
    read_document: "Read document",
    fetch_url: "Read page",
  };
  return labels[name] ?? name;
}

function previewArgs(value: unknown): string {
  if (!value || typeof value !== "object") return "";
  const record = value as Record<string, unknown>;
  const interesting = record.command ?? record.path ?? record.query ?? record.url ?? record.name ?? record.message;
  return typeof interesting === "string" ? interesting : "";
}

export function FindBar() {
  const findOpen = useUi((state) => state.findOpen);
  const findQuery = useUi((state) => state.findQuery);
  const findIndex = useUi((state) => state.findIndex);
  const setFind = useUi((state) => state.setFind);
  const setFindIndex = useUi((state) => state.setFindIndex);
  const messages = useSession((state) => state.messages);
  const t = useT();
  const count = useMemo(() => matchingIds(messages, findOpen ? findQuery.trim() : "").length, [findOpen, findQuery, messages]);
  if (!findOpen) return null;
  return (
    <div className="flex items-center gap-2 border-b border-line px-4 py-2">
      <input
        autoFocus
        className="w-56 rounded-md border border-line bg-panel-2 px-2 py-1 text-sm outline-none"
        placeholder={t("Find in chat")}
        value={findQuery}
        onChange={(event) => setFind(true, event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Escape") setFind(false, "");
          if (event.key === "Enter") setFindIndex(count === 0 ? 0 : (findIndex + 1) % count);
        }}
      />
      <span className="text-xs text-muted">{count === 0 ? "0" : `${findIndex + 1} / ${count}`}</span>
      <button className="text-xs text-muted" onClick={() => setFindIndex(count === 0 ? 0 : (findIndex - 1 + count) % count)}>
        {t("Previous")}
      </button>
      <button className="text-xs text-muted" onClick={() => setFindIndex(count === 0 ? 0 : (findIndex + 1) % count)}>
        {t("Next")}
      </button>
      <button className="ml-auto text-xs text-muted" onClick={() => setFind(false, "")}>
        {t("Close")}
      </button>
    </div>
  );
}

