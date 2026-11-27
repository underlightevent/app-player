import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { ClipboardCopy, Monitor, Moon, Sun } from "lucide-react";
import { diagnosticReport, logMessage } from "../core/api";
import type { ThemeSetting } from "../core/theme";
import { LANGUAGES, type Language } from "../i18n";
import { OnlineSettings } from "./OnlineSettings";

export interface SettingsProps {
  theme: ThemeSetting;
  onThemeChange: (t: ThemeSetting) => void;
  language: Language;
  onLanguageChange: (l: Language) => void;
  density: "compact" | "comfortable";
  keyNotation: "camelot" | "musical";
  onSettingChange: (key: string, value: string) => void;
  /** Valeurs brutes des réglages (écriture des tags…). */
  values: Record<string, string>;
  onExport: () => Promise<void>;
  onImport: () => Promise<void>;
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

function Segmented<T extends string>({ label, value, options, onChange }: {
  label: string;
  value: T;
  options: { id: T; label: string; icon?: ReactNode }[];
  onChange: (v: T) => void;
}) {
  return (
    <div className="segmented" role="group" aria-label={label}>
      {options.map((o) => (
        <button key={o.id} aria-pressed={value === o.id} onClick={() => onChange(o.id)}>
          {o.icon}
          {o.label}
        </button>
      ))}
    </div>
  );
}

export function SettingsView(p: SettingsProps) {
  const { t } = useTranslation();

  const copyDiagnostic = async () => {
    try {
      await copyText(await diagnosticReport());
      p.onToast(t("settings.diagnosticCopied"));
    } catch (e) {
      logMessage("error", `Copie du diagnostic : ${String(e)}`);
      p.onToast(t("settings.diagnosticFailed"));
    }
  };

  return (
    <div className="settings">
      <h1>{t("settings.title")}</h1>

      <section>
        <h2>{t("settings.appearance")}</h2>
        <p className="hint">{t("settings.appearanceHint")}</p>
        <Segmented
          label={t("settings.appearance")}
          value={p.theme}
          onChange={p.onThemeChange}
          options={[
            { id: "light", label: t("settings.themeLight"), icon: <Sun size={16} aria-hidden /> },
            { id: "dark", label: t("settings.themeDark"), icon: <Moon size={16} aria-hidden /> },
            { id: "system", label: t("settings.themeSystem"), icon: <Monitor size={16} aria-hidden /> },
          ]}
        />
      </section>

      <section>
        <h2>{t("settings.language")}</h2>
        <p className="hint">{t("settings.languageHint")}</p>
        <Segmented
          label={t("settings.language")}
          value={p.language}
          onChange={p.onLanguageChange}
          options={LANGUAGES.map((l) => ({ id: l, label: t(`languages.${l}`) }))}
        />
      </section>

      <section>
        <h2>{t("settings.density")}</h2>
        <p className="hint">{t("settings.densityHint")}</p>
        <Segmented
          label={t("settings.density")}
          value={p.density}
          onChange={(v) => p.onSettingChange("table_density", v)}
          options={[
            { id: "compact", label: t("settings.densityCompact") },
            { id: "comfortable", label: t("settings.densityComfortable") },
          ]}
        />
      </section>

      <section>
        <h2>{t("settings.keyNotation")}</h2>
        <p className="hint">{t("settings.keyNotationHint")}</p>
        <Segmented
          label={t("settings.keyNotation")}
          value={p.keyNotation}
          onChange={(v) => p.onSettingChange("key_notation", v)}
          options={[
            { id: "camelot", label: t("settings.keyCamelot") },
            { id: "musical", label: t("settings.keyMusical") },
          ]}
        />
      </section>

      <OnlineSettings values={p.values} onSettingChange={p.onSettingChange} onToast={p.onToast} />

      <section>
        <h2>{t("settings.writing")}</h2>
        <p className="hint">{t("settings.writingHint")}</p>
        <div className="checks">
          <label>
            <input type="checkbox" checked={p.values.version_in_title !== "false"} onChange={(e) => p.onSettingChange("version_in_title", String(e.target.checked))} />
            {t("settings.versionInTitle")}
          </label>
          <label>
            <input type="checkbox" checked={p.values.bpm_decimals === "true"} onChange={(e) => p.onSettingChange("bpm_decimals", String(e.target.checked))} />
            {t("settings.bpmDecimals")}
          </label>
          <label>
            <input type="checkbox" checked={p.values.single_cover !== "false"} onChange={(e) => p.onSettingChange("single_cover", String(e.target.checked))} />
            {t("settings.singleCover")}
          </label>
        </div>
        <h3 className="form-label">{t("settings.artistSeparator")}</h3>
        <Segmented
          label={t("settings.artistSeparator")}
          value={(p.values.artist_separator ?? "comma") as "comma" | "ampersand" | "semicolon"}
          onChange={(v) => p.onSettingChange("artist_separator", v)}
          options={[
            { id: "comma", label: "A, B" },
            { id: "ampersand", label: "A & B" },
            { id: "semicolon", label: "A; B" },
          ]}
        />
        <h3 className="form-label">{t("settings.coverSize")}</h3>
        <Segmented
          label={t("settings.coverSize")}
          value={(p.values.cover_size ?? "1000") as "500" | "600" | "1000" | "1400"}
          onChange={(v) => p.onSettingChange("cover_size", v)}
          options={["500", "600", "1000", "1400"].map((s) => ({ id: s as "500", label: `${s} px` }))}
        />
      </section>

      <section>
        <h2>{t("settings.quality")}</h2>
        <p className="hint">{t("settings.qualityHint")}</p>
        <h3 className="form-label">{t("settings.duplicateOrder")}</h3>
        <select
          className="text-input"
          value={p.values.duplicate_order ?? "wav,aiff,flac"}
          onChange={(e) => p.onSettingChange("duplicate_order", e.target.value)}
          aria-label={t("settings.duplicateOrder")}
        >
          {["wav,aiff,flac", "wav,flac,aiff", "aiff,wav,flac", "aiff,flac,wav", "flac,wav,aiff", "flac,aiff,wav"].map((o) => (
            <option key={o} value={o}>
              {o.split(",").map((f) => f.toUpperCase()).join(" > ")}
            </option>
          ))}
        </select>
        <p className="hint small">{t("settings.duplicateOrderHint")}</p>
        <h3 className="form-label">{t("settings.lowBitrate")}</h3>
        <Segmented
          label={t("settings.lowBitrate")}
          value={(p.values.low_bitrate ?? "320") as "320"}
          onChange={(v) => p.onSettingChange("low_bitrate", v)}
          options={["128", "160", "192", "256", "320"].map((s) => ({ id: s as "320", label: `${s} kbit/s` }))}
        />
      </section>

      <section>
        <h2>{t("settings.database")}</h2>
        <p className="hint">{t("settings.databaseHint")}</p>
        <div className="folder-actions">
          <button className="btn" onClick={() => void p.onExport()}>{t("settings.export")}</button>
          <button className="btn" onClick={() => void p.onImport()}>{t("settings.import")}</button>
        </div>
      </section>

      <section>
        <h2>{t("settings.diagnostic")}</h2>
        <p className="hint">{t("settings.diagnosticHint")}</p>
        <button className="btn" onClick={copyDiagnostic}>
          <ClipboardCopy size={16} aria-hidden />
          {t("settings.diagnosticCopy")}
        </button>
      </section>

      <p className="about tabular">{t("settings.about", { version: p.version })}</p>
    </div>
  );
}
