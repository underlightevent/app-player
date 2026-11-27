import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Modal } from "./Modal";
import { titleFormatApply, titleFormatPreview } from "../core/edit";

/** Présentations possibles du champ titre écrit dans les fichiers. */
const FORMATS = [
  { id: "title", version: false, artist: false },
  { id: "titleVersion", version: true, artist: false },
  { id: "artistTitle", version: false, artist: true },
  { id: "artistTitleVersion", version: true, artist: true },
] as const;

interface Props {
  values: Record<string, string>;
  onSettingChange: (key: string, value: string) => void;
  onToast: (msg: string, action?: { label: string; run: () => void }, kind?: "error") => void;
}

/** Réglage « Titre écrit dans les fichiers » (ce que les platines affichent dans les playlists). */
export function TitleFormatSettings(p: Props) {
  const { t, i18n } = useTranslation();
  const version = p.values.version_in_title !== "false";
  const artist = p.values.artist_in_title === "true";
  const current = FORMATS.find((f) => f.version === version && f.artist === artist)?.id ?? "titleVersion";
  const [preview, setPreview] = useState<{ count: number; examples: string[] } | null>(null);
  const [busy, setBusy] = useState(false);
  const nf = new Intl.NumberFormat(i18n.language);

  const choose = (f: (typeof FORMATS)[number]) => {
    if (f.version !== version) p.onSettingChange("version_in_title", String(f.version));
    if (f.artist !== artist) p.onSettingChange("artist_in_title", String(f.artist));
  };

  const openPreview = async () => {
    try {
      const [count, examples] = await titleFormatPreview();
      setPreview({ count, examples });
    } catch (e) {
      p.onToast(String(e), undefined, "error");
    }
  };

  const apply = async () => {
    setBusy(true);
    try {
      const r = await titleFormatApply();
      setPreview(null);
      p.onToast(
        r.failures.length > 0
          ? t("titleFormat.doneWithFailures", { count: r.failures.length })
          : t("titleFormat.done"),
        undefined,
        r.failures.length > 0 ? "error" : undefined,
      );
    } catch (e) {
      p.onToast(String(e), undefined, "error");
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <h3 className="form-label">{t("titleFormat.label")}</h3>
      <p className="hint">{t("titleFormat.hint")}</p>
      <div className="checks" role="radiogroup" aria-label={t("titleFormat.label")}>
        {FORMATS.map((f) => (
          <label key={f.id}>
            <input type="radio" name="title-format" checked={current === f.id} onChange={() => choose(f)} />
            {t(`titleFormat.options.${f.id}`)}
          </label>
        ))}
      </div>
      <p>
        <button className="btn" onClick={openPreview}>
          {t("titleFormat.applyButton")}
        </button>
      </p>
      {preview && (
        <Modal
          title={t("titleFormat.confirmTitle")}
          onClose={() => !busy && setPreview(null)}
          footer={
            <>
              <button className="btn" onClick={() => setPreview(null)} disabled={busy}>
                {t("common.cancel")}
              </button>
              <button className="btn btn-primary" onClick={apply} disabled={busy || preview.count === 0}>
                {t("titleFormat.confirm")}
              </button>
            </>
          }
        >
          <p>{t("titleFormat.confirmText", { count: preview.count, formatted: nf.format(preview.count) })}</p>
          {preview.examples.length > 0 && (
            <>
              <p className="muted">{t("titleFormat.examples")}</p>
              <ul>
                {preview.examples.map((ex, i) => (
                  <li key={i}>{ex}</li>
                ))}
              </ul>
            </>
          )}
          <p className="hint">{t("titleFormat.confirmHint")}</p>
        </Modal>
      )}
    </>
  );
}
