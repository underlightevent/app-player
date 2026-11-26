import { useTranslation } from "react-i18next";
import { Music, Play, SkipBack, SkipForward, Volume2 } from "lucide-react";

export function PlayerBar() {
  const { t } = useTranslation();
  return (
    <footer className="player" aria-label={t("player.nothing")}>
      <div className="player-cover" aria-hidden>
        <Music size={20} />
      </div>
      <div className="player-info">{t("player.nothing")}</div>
      <div className="player-controls">
        <button className="icon-btn" disabled aria-label={t("player.previous")} title={t("player.previous")}>
          <SkipBack size={20} aria-hidden />
        </button>
        <button className="icon-btn" disabled aria-label={t("player.play")} title={t("player.play")}>
          <Play size={20} aria-hidden />
        </button>
        <button className="icon-btn" disabled aria-label={t("player.next")} title={t("player.next")}>
          <SkipForward size={20} aria-hidden />
        </button>
      </div>
      <div className="waveform">
        <span className="time tabular">0:00</span>
        <div className="waveform-line" />
        <span className="time tabular">0:00</span>
      </div>
      <button className="icon-btn" disabled aria-label={t("player.volume")} title={t("player.volume")}>
        <Volume2 size={20} aria-hidden />
      </button>
    </footer>
  );
}
