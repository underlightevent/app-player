import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Check, Folder, FolderInput, Globe, Monitor, Moon, Sun } from "lucide-react";
import type { ThemeSetting } from "../core/theme";
import { LANGUAGES, type Language } from "../i18n";
import { libraryAddRoot, pickFolder } from "../core/library";

interface Props {
  theme: ThemeSetting;
  onThemeChange: (t: ThemeSetting) => void;
  language: Language;
  onLanguageChange: (l: Language) => void;
  onFinish: () => void;
}

/** Premier lancement : 3 écrans (langue et thème ; dossiers ; recherche en ligne et Rekordbox). */
export function Onboarding(p: Props) {
  const { t } = useTranslation();
  const [step, setStep] = useState(0);
  const [library, setLibrary] = useState<string | null>(null);
  const [inbox, setInbox] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const choose = async (kind: "library" | "inbox") => {
    const path = await pickFolder(t(kind === "library" ? "views.library.pickTitle" : "onboarding.inboxPick"));
    if (!path) return;
    try {
      await libraryAddRoot(path, kind);
      setError(null);
      if (kind === "library") setLibrary(path);
      else setInbox(path);
    } catch {
      setError(t("library.notAFolder"));
    }
  };

  return (
    <div className="onboarding" role="dialog" aria-modal aria-labelledby="ob-title">
      <div className="onboarding-card">
        <div className="onboarding-steps" aria-hidden>
          {[0, 1, 2].map((i) => <span key={i} className={i <= step ? "on" : ""} />)}
        </div>

        {step === 0 && (
          <>
            <h1 id="ob-title">{t("onboarding.welcome")}</h1>
            <p className="hint">{t("onboarding.welcomeText")}</p>
            <h2>{t("settings.language")}</h2>
            <div className="segmented">
              {LANGUAGES.map((l) => (
                <button key={l} aria-pressed={p.language === l} onClick={() => p.onLanguageChange(l)} lang={l}>{t(`languages.${l}`)}</button>
              ))}
            </div>
            <h2>{t("settings.appearance")}</h2>
            <div className="segmented">
              {([["light", Sun], ["dark", Moon], ["system", Monitor]] as const).map(([id, Icon]) => (
                <button key={id} aria-pressed={p.theme === id} onClick={() => p.onThemeChange(id)}>
                  <Icon size={16} aria-hidden /> {t(`settings.theme${id[0].toUpperCase()}${id.slice(1)}`)}
                </button>
              ))}
            </div>
          </>
        )}

        {step === 1 && (
          <>
            <h1 id="ob-title">{t("onboarding.foldersTitle")}</h1>
            <p className="hint">{t("onboarding.foldersText")}</p>
            <div className="ob-choice">
              <Folder size={20} aria-hidden />
              <div>
                <strong>{t("onboarding.library")}</strong>
                <div className="hint small">{library ?? t("onboarding.libraryHint")}</div>
              </div>
              <button className={library ? "btn" : "btn btn-primary"} onClick={() => void choose("library")}>
                {library ? <Check size={16} aria-hidden /> : null}
                {library ? t("onboarding.change") : t("onboarding.choose")}
              </button>
            </div>
            <div className="ob-choice">
              <FolderInput size={20} aria-hidden />
              <div>
                <strong>{t("onboarding.inbox")}</strong>
                <div className="hint small">{inbox ?? t("onboarding.inboxHint")}</div>
              </div>
              <button className="btn" onClick={() => void choose("inbox")}>
                {inbox ? <Check size={16} aria-hidden /> : null}
                {inbox ? t("onboarding.change") : t("onboarding.choose")}
              </button>
            </div>
            <p className="hint small">{t("onboarding.exfat")}</p>
            {error && <p className="text-error small">{error}</p>}
          </>
        )}

        {step === 2 && (
          <>
            <h1 id="ob-title">{t("onboarding.onlineTitle")}</h1>
            <div className="ob-choice">
              <Globe size={20} aria-hidden />
              <div>
                <strong>{t("onboarding.online")}</strong>
                <div className="hint small">{t("onboarding.onlineText")}</div>
              </div>
            </div>
            <h2>{t("onboarding.rekordboxTitle")}</h2>
            <ul className="ob-tips">
              <li>{t("onboarding.tip1")}</li>
              <li>{t("onboarding.tip2")}</li>
              <li>{t("onboarding.tip3")}</li>
            </ul>
          </>
        )}

        <div className="onboarding-actions">
          {step > 0 && <button className="btn" onClick={() => setStep((s) => s - 1)}>{t("onboarding.back")}</button>}
          <span style={{ flex: 1 }} />
          {step < 2 ? (
            <button className="btn btn-primary" onClick={() => setStep((s) => s + 1)}>{t("onboarding.next")}</button>
          ) : (
            <button className="btn btn-primary" onClick={p.onFinish}>{t("onboarding.finish")}</button>
          )}
        </div>
      </div>
    </div>
  );
}
