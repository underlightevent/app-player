import { useCallback, useEffect, useRef, useState } from "react";
import {
  libraryQuery,
  libraryRoots,
  libraryRows,
  libraryVolumes,
  onEvent,
  type LibraryQuery,
  type RootInfo,
  type TrackRow,
  type VolumeStatus,
} from "./library";
import { logMessage } from "./api";

/**
 * Données du tableau : liste ordonnée des identifiants, lignes chargées à la demande,
 * rafraîchissement automatique quand le cœur signale un changement.
 *
 * Les lignes sont rangées dans une table qui n'est jamais recopiée (100 000 morceaux) ;
 * un compteur `rowsVersion` signale à l'affichage qu'elle a changé.
 */
export function useLibrary(query: LibraryQuery) {
  const [ids, setIds] = useState<number[]>([]);
  const rowsRef = useRef(new Map<number, TrackRow>());
  // Lignes de la version précédente, affichées le temps que les nouvelles arrivent (pas de clignotement).
  const staleRef = useRef(new Map<number, TrackRow>());
  const [rowsVersion, setRowsVersion] = useState(0);
  const [loading, setLoading] = useState(true);
  const [roots, setRoots] = useState<RootInfo[]>([]);
  const [volumes, setVolumes] = useState<VolumeStatus[]>([]);
  const inflight = useRef(new Set<number>());
  const generation = useRef(0);
  const queryRef = useRef(query);
  queryRef.current = query;

  const refresh = useCallback(async () => {
    const gen = ++generation.current;
    try {
      const [list, r, v] = await Promise.all([libraryQuery(queryRef.current), libraryRoots(), libraryVolumes()]);
      if (gen !== generation.current) return; // une requête plus récente a été lancée
      // Les lignes connues restent affichées, mais seront relues.
      inflight.current.clear();
      if (rowsRef.current.size) staleRef.current = rowsRef.current;
      rowsRef.current = new Map();
      setIds(list);
      setRoots(r);
      setVolumes(v);
      setRowsVersion((n) => n + 1);
    } catch (e) {
      logMessage("error", `Bibliothèque : ${JSON.stringify(e)}`);
    } finally {
      if (gen === generation.current) setLoading(false);
    }
  }, []);

  // Nouvelle requête (recherche, tri, filtres) : légère attente pendant la frappe.
  const key = JSON.stringify(query);
  useEffect(() => {
    const h = window.setTimeout(() => void refresh(), query.search ? 120 : 0);
    return () => window.clearTimeout(h);
  }, [key, refresh, query.search]);

  // Changements signalés par le cœur (indexation, disque branché…).
  useEffect(() => {
    let timer: number | undefined;
    const schedule = () => {
      window.clearTimeout(timer);
      timer = window.setTimeout(() => void refresh(), 300);
    };
    const offA = onEvent("library:changed", schedule);
    const offB = onEvent("library:volumes", schedule);
    return () => {
      offA();
      offB();
      window.clearTimeout(timer);
    };
  }, [refresh]);

  const needRows = useCallback(async (wanted: number[]) => {
    const todo = wanted.filter((id) => !inflight.current.has(id));
    if (!todo.length) return;
    todo.forEach((id) => inflight.current.add(id));
    const gen = generation.current;
    try {
      for (let i = 0; i < todo.length; i += 400) {
        const chunk = await libraryRows(todo.slice(i, i + 400));
        if (gen !== generation.current) return;
        for (const r of chunk) rowsRef.current.set(r.id, r);
        setRowsVersion((n) => n + 1);
      }
    } catch (e) {
      todo.forEach((id) => inflight.current.delete(id));
      logMessage("error", `Lignes : ${JSON.stringify(e)}`);
    }
  }, []);

  return { ids, rows: rowsRef.current, staleRows: staleRef.current, rowsVersion, needRows, refresh, loading, roots, volumes };
}
