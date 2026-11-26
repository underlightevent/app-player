import { describe, expect, it } from "vitest";
import { formatDuration, formatQuality, isLowBitrate, toCamelot } from "./format";

describe("mise en forme", () => {
  it("durées", () => {
    expect(formatDuration(0)).toBe("0:00");
    expect(formatDuration(65_000)).toBe("1:05");
    expect(formatDuration(3_725_000)).toBe("1:02:05");
    expect(formatDuration(null)).toBe("");
  });

  it("Camelot", () => {
    expect(toCamelot("Am")).toBe("8A");
    expect(toCamelot("C")).toBe("8B");
    expect(toCamelot("F#m")).toBe("11A");
    expect(toCamelot("Db")).toBe("3B");
    expect(toCamelot("Ebm")).toBe("2A");
  });

  it("qualité", () => {
    const base = { codec: null, bitDepth: null, sampleRate: null };
    expect(formatQuality({ ...base, format: "mp3", bitrate: 320 }, "fr")).toBe("MP3 320");
    expect(formatQuality({ ...base, format: "wav", bitrate: 1411, bitDepth: 24, sampleRate: 44100 }, "fr")).toBe("WAV 24/44,1");
    expect(formatQuality({ ...base, format: "wav", bitrate: 1411, bitDepth: 24, sampleRate: 44100 }, "en")).toBe("WAV 24/44.1");
    expect(isLowBitrate({ format: "mp3", bitrate: 192 })).toBe(true);
    expect(isLowBitrate({ format: "mp3", bitrate: 320 })).toBe(false);
    expect(isLowBitrate({ format: "wav", bitrate: 1411 })).toBe(false);
  });
});
