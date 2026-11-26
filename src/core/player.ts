/** Lecteur audio : ordres envoyés au cœur, état reçu par l'événement « player:state ». */
import { invoke } from "@tauri-apps/api/core";
import { inTauri } from "./api";

export interface PlayerState {
  trackId: number | null;
  playing: boolean;
  positionMs: number;
  durationMs: number;
  volume: number;
  ended: boolean;
  error: string | null;
}

export const initialPlayerState: PlayerState = {
  trackId: null,
  playing: false,
  positionMs: 0,
  durationMs: 0,
  volume: 0.8,
  ended: false,
  error: null,
};

export async function playerLoad(id: number, autoplay = true): Promise<void> {
  if (!inTauri) return;
  await invoke("player_load", { id, autoplay });
}

export function playerCommand(action: "play" | "pause" | "stop" | "seek" | "volume", value?: number) {
  if (!inTauri) return;
  void invoke("player_command", { action, value: value ?? null });
}

export async function playerState(): Promise<PlayerState> {
  if (!inTauri) return initialPlayerState;
  return invoke<PlayerState>("player_state");
}

export async function waveform(id: number): Promise<number[] | null> {
  if (!inTauri) {
    // Aperçu navigateur : onde factice
    return Array.from({ length: 1000 }, (_, i) => Math.round(120 + 100 * Math.sin(i / 23) * Math.cos(i / 97)));
  }
  return invoke<number[] | null>("waveform", { id });
}
