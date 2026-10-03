import { useState } from "react";

import { BUILTIN_SKILLS, type Skill } from "../lib/catalog";
import { useT } from "../lib/i18n";
import { useUi } from "../stores/ui";

const empty: Skill = { id: "", name: "", description: "", body: "" };

export function SkillsView() {
  const skills = useUi((state) => state.skills);
  const saveSkill = useUi((state) => state.saveSkill);
  const removeSkill = useUi((state) => state.removeSkill);
  const [draft, setDraft] = useState<Skill>(empty);
  const t = useT();

  function edit(skill: Skill) {
    setDraft(skill.builtin ? { ...skill, id: crypto.randomUUID(), builtin: false } : skill);
  }

  return (
    <div className="scroll-thin flex-1 overflow-auto px-8 py-6">
      <div className="mx-auto grid max-w-4xl gap-8 md:grid-cols-[240px_1fr]">
        <div>
          <h1 className="text-xl font-medium">{t("Skills")}</h1>
          <p className="mt-2 text-sm text-muted">{t("Type $name in chat to attach those instructions to the next run.")}</p>
          <div className="mt-4 space-y-1">
            {BUILTIN_SKILLS.map((skill) => (
              <button key={skill.id} className="block w-full rounded-md px-2 py-1.5 text-left text-sm hover:bg-elev" onClick={() => edit(skill)}>
                ${skill.name}
                <div className="text-xs text-muted">{t(skill.description)}</div>
              </button>
            ))}
            {skills.map((skill) => (
              <button key={skill.id} className="block w-full rounded-md px-2 py-1.5 text-left text-sm hover:bg-elev" onClick={() => edit(skill)}>
                ${skill.name}
                <div className="text-xs text-muted">{skill.description}</div>
              </button>
            ))}
          </div>
        </div>
        <form
          className="space-y-3"
          onSubmit={(event) => {
            event.preventDefault();
            const name = draft.name.trim().replace(/\s+/g, "-");
            if (!name || !draft.body.trim()) return;
            saveSkill({ ...draft, id: draft.id || crypto.randomUUID(), name, builtin: false });
            setDraft(empty);
          }}
        >
          <label className="block text-sm">
            <div className="mb-1 text-muted">{t("Name")}</div>
            <input className="field" value={draft.name} onChange={(event) => setDraft({ ...draft, name: event.target.value })} />
          </label>
          <label className="block text-sm">
            <div className="mb-1 text-muted">{t("Description")}</div>
            <input className="field" value={draft.description} onChange={(event) => setDraft({ ...draft, description: event.target.value })} />
          </label>
          <label className="block text-sm">
            <div className="mb-1 text-muted">{t("Instructions")}</div>
            <textarea className="field h-48 font-mono text-xs" value={draft.body} onChange={(event) => setDraft({ ...draft, body: event.target.value })} />
          </label>
          <div className="flex gap-2">
            <button className="rounded-full bg-white px-4 py-2 text-sm text-black" type="submit">
              {t("Save")}
            </button>
            {draft.id && skills.some((skill) => skill.id === draft.id) ? (
              <button
                className="rounded-full border border-line px-4 py-2 text-sm"
                type="button"
                onClick={() => {
                  removeSkill(draft.id);
                  setDraft(empty);
                }}
              >
                {t("Delete")}
              </button>
            ) : null}
          </div>
        </form>
      </div>
    </div>
  );
}
