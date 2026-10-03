import { check, type Update } from "@tauri-apps/plugin-updater";

import { rawMessage } from "./api";
import { translate } from "./i18n";
import { useUi } from "../stores/ui";

export async function findUpdate(): Promise<Update | null> {
  return check();
}

export function updateError(error: unknown): string {
  const message = rawMessage(error);
  const language = useUi.getState().language;
  if (/fetch|network|endpoint|release|timed out|offline|404/i.test(message)) {
    return translate(language, "Could not load update information. There may be no release, or the network is unavailable.");
  }
  return translate(language, message);
}
