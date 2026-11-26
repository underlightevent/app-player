import { useTranslation } from "react-i18next";
import { Disc3 } from "lucide-react";

export function DetailPanel() {
  const { t } = useTranslation();
  return (
    <aside className="detail" aria-label={t("detail.title")}>
      <h2 className="detail-title">{t("detail.title")}</h2>
      <div className="cover-placeholder" aria-hidden>
        <Disc3 size={48} strokeWidth={1.25} />
      </div>
      <p>{t("detail.empty")}</p>
      <p className="muted">{t("detail.hint")}</p>
    </aside>
  );
}
