import { useEffect, useRef, useState, type KeyboardEvent, type MouseEvent } from "react";
import { useTranslation } from "react-i18next";
import { Music, Pause, Play, SkipBack, SkipForward, Volume1, Volume2, VolumeX } from "lucide-react";
import type { TrackRow } from "../core/library";
import type { PlayerState } from "../core/player";
import { waveform } from "../core/player";
import { trackCover } from "../core/edit";
import { formatDuration } from "../core/format";

interface Props {
  state: PlayerState;
  track: TrackRow | null;
  onToggle: () => void;
  onPrev: () => void;
  onNext: () => void;
  onSeek: (ms: number) => void;
  onVolume: (v: number) => void;
}

/** Forme d'onde cliquable, dessinée sur un canevas (rapide, même sur des écrans larges). */
function Waveform({ trackId, position, duration, onSeek }: { trackId: number | null; position: number; duration: number; onSeek: (ms: number) => void }) {
  const { t } = useTranslation();
  const canvas = useRef<HTMLCanvasElement>(null);
  const [peaks, setPeaks] = useState<number[] | null>(null);
  const [hover, setHover] = useState<number | null>(null);

  useEffect(() => {
    setPeaks(null);
    if (trackId == null) return;
    let stale = false;
    void waveform(trackId).then((w) => !stale && setPeaks(w));
    return () => {
      stale = true;
    };
  }, [trackId]);

  // Dessin : partie jouée en couleur d'accent, le reste en couleur de bordure.
  useEffect(() => {
    const c = canvas.current;
    if (!c) return;
    const dpr = window.devicePixelRatio || 1;
    const w = c.clientWidth;
    const h = c.clientHeight;
    if (c.width !== Math.round(w * dpr) || c.height !== Math.round(h * dpr)) {
      c.width = Math.round(w * dpr);
      c.height = Math.round(h * dpr);
    }
    const ctx = c.getContext("2d");
    if (!ctx) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);
    const styles = getComputedStyle(c);
    const played = styles.getPropertyValue("--color-accent").trim() || "#4f46e5";
    const rest = styles.getPropertyValue("--color-border").trim() || "#e7e5e4";
    const ratio = duration > 0 ? position / duration : 0;
    const bars = Math.max(1, Math.floor(w / 3));
    for (let i = 0; i < bars; i++) {
      const x = i * 3;
      const v = peaks ? peaks[Math.floor((i / bars) * peaks.length)] / 255 : 0.08;
      const bh = Math.max(2, v * (h - 4));
      ctx.fillStyle = i / bars < ratio ? played : rest;
      ctx.fillRect(x, (h - bh) / 2, 2, bh);
    }
    if (hover != null) {
      ctx.fillStyle = styles.getPropertyValue("--color-text").trim() || "#000";
      ctx.fillRect(hover * w, 0, 1, h);
    }
  }, [peaks, position, duration, hover]);

  const ratioAt = (e: MouseEvent<HTMLCanvasElement>) => {
    const r = e.currentTarget.getBoundingClientRect();
    return Math.min(1, Math.max(0, (e.clientX - r.left) / r.width));
  };
  const onKey = (e: KeyboardEvent) => {
    if (e.key === "ArrowLeft") onSeek(Math.max(0, position - 10_000));
    else if (e.key === "ArrowRight") onSeek(Math.min(duration, position + 10_000));
    else return;
    e.preventDefault();
    e.stopPropagation();
  };
  return (
    <canvas
      ref={canvas}
      className="waveform-canvas"
      role="slider"
      tabIndex={trackId == null ? -1 : 0}
      aria-label={t("player.position")}
      aria-valuemin={0}
      aria-valuemax={Math.round(duration / 1000)}
      aria-valuenow={Math.round(position / 1000)}
      aria-valuetext={`${formatDuration(position)} / ${formatDuration(duration)}`}
      onClick={(e) => trackId != null && duration > 0 && onSeek(ratioAt(e) * duration)}
      onMouseMove={(e) => setHover(ratioAt(e))}
      onMouseLeave={() => setHover(null)}
      onKeyDown={onKey}
    />
  );
}

export function PlayerBar({ state, track, onToggle, onPrev, onNext, onSeek, onVolume }: Props) {
  const { t } = useTranslation();
  const [cover, setCover] = useState<string | null>(null);
  const [showVolume, setShowVolume] = useState(false);
  useEffect(() => {
    setCover(null);
    if (!track || track.coverCount === 0) return;
    let stale = false;
    void trackCover(track.id, 96).then((c) => !stale && setCover(c));
    return () => {
      stale = true;
    };
  }, [track?.id, track?.coverCount]);

  const VolIcon = state.volume === 0 ? VolumeX : state.volume < 0.5 ? Volume1 : Volume2;
  const has = state.trackId != null;
  return (
    <footer className="player" aria-label={t("player.label")}>
      <div className="player-cover" aria-hidden>
        {cover ? <img src={cover} alt="" /> : <Music size={20} />}
      </div>
      <div className="player-info">
        {track ? (
          <>
            <div className="player-title" title={track.title ?? track.fileName}>
              {track.title ?? track.fileName}
              {track.version && <span className="muted"> ({track.version})</span>}
            </div>
            <div className="player-artist muted">{track.artists.join(", ")}</div>
          </>
        ) : (
          <span className="muted">{state.error ? t(`player.errors.${state.error}`, { defaultValue: t("player.errors.unreadable") }) : t("player.nothing")}</span>
        )}
      </div>
      <div className="player-controls">
        <button className="icon-btn" onClick={onPrev} disabled={!has} aria-label={t("player.previous")} title={`${t("player.previous")} (Maj+←)`}>
          <SkipBack size={20} aria-hidden />
        </button>
        <button className="icon-btn play-btn" onClick={onToggle} disabled={!has} aria-label={state.playing ? t("player.pause") : t("player.play")} title={`${state.playing ? t("player.pause") : t("player.play")} (${t("player.space")})`}>
          {state.playing ? <Pause size={20} aria-hidden /> : <Play size={20} aria-hidden />}
        </button>
        <button className="icon-btn" onClick={onNext} disabled={!has} aria-label={t("player.next")} title={`${t("player.next")} (Maj+→)`}>
          <SkipForward size={20} aria-hidden />
        </button>
      </div>
      <div className="waveform">
        <span className="time tabular">{formatDuration(state.positionMs)}</span>
        <Waveform trackId={state.trackId} position={state.positionMs} duration={state.durationMs} onSeek={onSeek} />
        <span className="time tabular">{formatDuration(state.durationMs)}</span>
      </div>
      <div className="popover-anchor volume">
        <button className="icon-btn" onClick={() => setShowVolume((v) => !v)} aria-label={t("player.volume")} aria-expanded={showVolume} title={t("player.volume")}>
          <VolIcon size={20} aria-hidden />
        </button>
        {showVolume && (
          <>
            <div className="menu-backdrop" onClick={() => setShowVolume(false)} />
            <div className="volume-popover">
              <input
                type="range"
                min={0}
                max={100}
                value={Math.round(state.volume * 100)}
                onChange={(e) => onVolume(Number(e.target.value) / 100)}
                aria-label={t("player.volume")}
              />
            </div>
          </>
        )}
      </div>
    </footer>
  );
}
