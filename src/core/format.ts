/** Mise en forme des valeurs affichées (durées, qualité, tonalités…). */

export function formatDuration(ms: number | null | undefined): string {
  if (ms == null || !Number.isFinite(ms)) return "";
  const total = Math.round(ms / 1000);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const ss = String(s).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${ss}` : `${m}:${ss}`;
}

export function formatSize(bytes: number, locale: string): string {
  const units = ["o", "Ko", "Mo", "Go"];
  const unitsEn = ["B", "KB", "MB", "GB"];
  let v = bytes;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  const n = new Intl.NumberFormat(locale, { maximumFractionDigits: i === 0 ? 0 : 1 }).format(v);
  return `${n} ${(locale.startsWith("fr") ? units : unitsEn)[i]}`;
}

const LOSSLESS = new Set(["wav", "aiff", "flac"]);

/** « WAV 24/48 », « MP3 320 », « FLAC 16/44,1 ». */
export function formatQuality(
  r: { format: string; bitrate: number | null; bitDepth: number | null; sampleRate: number | null; codec: string | null },
  locale: string,
): string {
  const fmt = r.format.toUpperCase();
  const lossless = LOSSLESS.has(r.format) || r.codec === "ALAC";
  if (lossless && r.bitDepth && r.sampleRate) {
    const khz = new Intl.NumberFormat(locale, { maximumFractionDigits: 1 }).format(r.sampleRate / 1000);
    return `${fmt} ${r.bitDepth}/${khz}`;
  }
  if (r.bitrate) {
    // lofty donne le débit en kbit/s ; certaines sources en bit/s
    const kbps = r.bitrate > 10_000 ? Math.round(r.bitrate / 1000) : r.bitrate;
    return `${fmt} ${kbps}`;
  }
  return fmt;
}

export function isLowBitrate(r: { format: string; bitrate: number | null }, threshold = 320): boolean {
  if (LOSSLESS.has(r.format) || !r.bitrate) return false;
  const kbps = r.bitrate > 10_000 ? r.bitrate / 1000 : r.bitrate;
  return kbps < threshold - 5;
}

// ---------- Tonalités (stockées en notation musicale, affichées en Camelot par défaut) ----------
const NOTE_INDEX: Record<string, number> = { C: 0, D: 2, E: 4, F: 5, G: 7, A: 9, B: 11 };
const CAMELOT_MAJOR = [8, 3, 10, 5, 12, 7, 2, 9, 4, 11, 6, 1];
const CAMELOT_MINOR = [5, 12, 7, 2, 9, 4, 11, 6, 1, 8, 3, 10];

export function toCamelot(key: string | null | undefined): string {
  if (!key) return "";
  const m = /^([A-Ga-g])([#b]?)(m?)$/.exec(key.trim());
  if (!m) return key;
  let idx = NOTE_INDEX[m[1].toUpperCase()];
  if (m[2] === "#") idx += 1;
  if (m[2] === "b") idx -= 1;
  idx = (idx + 12) % 12;
  return m[3] ? `${CAMELOT_MINOR[idx]}A` : `${CAMELOT_MAJOR[idx]}B`;
}

export function formatKey(key: string | null | undefined, notation: "camelot" | "musical"): string {
  if (!key) return "";
  return notation === "camelot" ? toCamelot(key) : key;
}

export function formatBpm(bpm: number | null | undefined, locale: string): string {
  if (bpm == null) return "";
  return new Intl.NumberFormat(locale, { maximumFractionDigits: 0 }).format(bpm);
}
