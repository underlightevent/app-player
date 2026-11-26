import { useTranslation } from "react-i18next";
import { ClipboardCopy, Monitor, Moon, Sun } from "lucide-react";
import { diagnosticReport, logMessage } from "../core/api";
import type { ThemeSetting } from "../core/theme";
import { LANGUAGES, type Language } from "../i18n";

interface Props {
  theme: ThemeSetting;
  onThemeChange: (t: ThemeSetting) => void;
  language: Language;
  onLanguageChange: (l: Language) => void;
  version: string;
  onToast: (msg: string) => void;
}

async function copyText(text: string) {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    // Solution de secours si l'accès au presse-papiers est refusé.
    const area = document.createElement("textarea");
    area.value = text;
    document.body.appendChild(area);
    area.select();
    const ok = document.execCommand("copy");
    area.remove();
    if (!ok) throw new Error("copie refusée");
  }
}

export function SettingsView({ theme, onThemeChange, language, onLanguageChange, version, onToast }: Props) {
  const { t } = useTranslation();

  const themes: { id: ThemeSetting; label: string; icon: typeof Sun }[] = [
    { id: "light", label: t("settings.themeLight"), icon: Sun },
    { id: "dark", label: t("settings.themeDark"), icon: Moon },
    { id: "system", label: t("settings.themeSystem"), icon: Monitor },
  ];

  const copyDiagnostic = async () => {
    try {
      await copyText(await diagnosticReport());
      onToast(t("settings.diagnosticCopied"));
    } catch (e) {
      logMessage("error", `Copie du diagnostic : ${String(e)}`);
      onToast(t("settings.diagnosticFailed"));
    }
  };

  return (
    <div className="settings">
      <h1>{t("settings.title")}</h1>

      <section aria-labelledby="s-appearance">
        <h2 id="s-appearance">{t("settings.appearance")}</h2>
        <p className="hint">{t("settings.appearanceHint")}</p>
        <div className="segmented" role="group" aria-labelledby="s-appearance">
          {themes.map(({ id, label, icon: Icon }) => (
            <button key={id} aria-pressed={theme === id} onClick={() => onThemeChange(id)}>
              <Icon size={16} aria-hidden />
              {label}
            </button>
          ))}
        </div>
      </section>

      <section aria-labelledby="s-language">
        <h2 id="s-language">{t("settings.language")}</h2>
        <p className="hint">{t("settings.languageHint")}</p>
        <div className="segmented" role="group" aria-labelledby="s-language">
          {LANGUAGES.map((lang) => (
            <button key={lang} aria-pressed={language === lang} onClick={() => onLanguageChange(lang)} lang={lang}>
              {t(`languages.${lang}`)}
            </button>
          ))}
        </div>
      </section>

      <section aria-labelledby="s-diag">
        <h2 id="s-diag">{t("settings.diagnostic")}</h2>
        <p className="hint">{t("settings.diagnosticHint")}</p>
        <button className="btn" onClick={copyDiagnostic}>
          <ClipboardCopy size={16} aria-hidden />
          {t("settings.diagnosticCopy")}
        </button>
      </section>

      <p className="about tabular">{t("settings.about", { version })}</p>
    </div>
  );
}
