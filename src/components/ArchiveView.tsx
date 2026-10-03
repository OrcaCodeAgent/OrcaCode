import { api, explain } from "../lib/api";
import { localizeTitle, useT } from "../lib/i18n";
import { isRunning, useSession } from "../stores/session";
import { useUi } from "../stores/ui";

export function ArchiveView() {
  const conversations = useSession((state) => state.conversations);
  const archived = useUi((state) => state.archived);
  const toggleArchive = useUi((state) => state.toggleArchive);
  const openConversation = useSession((state) => state.openConversation);
  const setView = useSession((state) => state.setView);
  const setBanner = useSession((state) => state.setBanner);
  const agentState = useSession((state) => state.agentState);
  const items = conversations.filter((conversation) => archived.includes(conversation.id));
  const running = isRunning(agentState);
  const t = useT();
  const language = useUi((state) => state.language);

  return (
    <div className="scroll-thin flex-1 overflow-auto px-8 py-6">
      <div className="mx-auto max-w-xl">
        <h1 className="text-xl font-medium">{t("Archive")}</h1>
        <p className="mt-2 text-sm text-muted">{t("Archived chats. Opening one brings it back to the recent list.")}</p>
        <div className="mt-5 space-y-1">
          {items.length === 0 ? <p className="text-sm text-muted">{t("No archived chats.")}</p> : null}
          {items.map((conversation) => (
            <div key={conversation.id} className="flex items-center gap-2 rounded-lg px-2 py-2 hover:bg-elev">
              <button
                className="min-w-0 flex-1 truncate text-left text-sm"
                onClick={() => {
                  if (running) return;
                  void api
                    .getConversation(conversation.id)
                    .then((detail) => {
                      if (!detail) return;
                      toggleArchive(conversation.id);
                      openConversation(detail);
                      setView("chat");
                    })
                    .catch((error) => setBanner(explain(error)));
                }}
              >
                {localizeTitle(language, conversation.title)}
              </button>
              <button className="text-xs text-muted" onClick={() => toggleArchive(conversation.id)}>
                {t("Restore")}
              </button>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
