/**
 * Glisser-déposer interne, à la souris.
 *
 * Sous Windows, Tauri intercepte le glisser-déposer du système (pour recevoir les fichiers
 * venus de l'Explorateur) : le glisser-déposer HTML5 ne fonctionne alors pas dans la
 * fenêtre. Ce petit module le remplace pour les déplacements internes (morceaux vers un
 * dossier, ordre des colonnes, ordre des artistes).
 */
import { useEffect, useRef, type MouseEvent as ReactMouseEvent, type RefObject } from "react";

export type Payload =
  | { kind: "tracks"; ids: number[] }
  | { kind: "column"; id: string }
  | { kind: "artist"; index: number }
  | { kind: "setRow"; id: number };

interface Target {
  kind: Payload["kind"];
  onDrop: (p: Payload) => void;
}

const targets = new Map<HTMLElement, Target>();
let dragging: Payload | null = null;

export function isDragging(kind?: Payload["kind"]): boolean {
  return dragging != null && (kind == null || dragging.kind === kind);
}

function targetAt(x: number, y: number, kind: Payload["kind"]): HTMLElement | null {
  let el = document.elementFromPoint(x, y) as HTMLElement | null;
  while (el) {
    const t = targets.get(el);
    if (t && t.kind === kind) return el;
    el = el.parentElement;
  }
  return null;
}

let justDragged = false;

/** Vrai juste après un glisser-déposer : le clic qui suit le relâchement doit être ignoré. */
export function consumeDragClick(): boolean {
  const v = justDragged;
  justDragged = false;
  return v;
}

/** À appeler sur mousedown : le glissement commence après 5 pixels de mouvement.
 *  `onNoDrag` est appelé si la souris est relâchée sans avoir glissé. */
export function startDrag(e: ReactMouseEvent, payload: () => Payload | null, label: () => string, onNoDrag?: () => void) {
  if (e.button !== 0) return;
  const startX = e.clientX;
  const startY = e.clientY;
  let ghost: HTMLDivElement | null = null;
  let over: HTMLElement | null = null;
  let p: Payload | null = null;

  const move = (ev: MouseEvent) => {
    if (!ghost) {
      if (Math.abs(ev.clientX - startX) + Math.abs(ev.clientY - startY) < 6) return;
      p = payload();
      if (!p) return cleanup();
      dragging = p;
      ghost = document.createElement("div");
      ghost.className = "drag-ghost";
      ghost.textContent = label();
      document.body.appendChild(ghost);
      document.body.classList.add("is-dragging");
    }
    ev.preventDefault();
    ghost.style.transform = `translate(${ev.clientX + 12}px, ${ev.clientY + 8}px)`;
    const t = targetAt(ev.clientX, ev.clientY, p!.kind);
    if (t !== over) {
      over?.classList.remove("drop-over");
      t?.classList.add("drop-over");
      over = t;
    }
  };
  const up = (ev: MouseEvent) => {
    if (ghost && p) {
      justDragged = true;
      window.setTimeout(() => (justDragged = false), 0);
      const t = targetAt(ev.clientX, ev.clientY, p.kind);
      if (t) targets.get(t)?.onDrop(p);
    } else {
      onNoDrag?.();
    }
    cleanup();
  };
  const key = (ev: KeyboardEvent) => {
    if (ev.key === "Escape") cleanup();
  };
  function cleanup() {
    window.removeEventListener("mousemove", move);
    window.removeEventListener("mouseup", up);
    window.removeEventListener("keydown", key, true);
    over?.classList.remove("drop-over");
    ghost?.remove();
    document.body.classList.remove("is-dragging");
    dragging = null;
  }
  window.addEventListener("mousemove", move);
  window.addEventListener("mouseup", up);
  window.addEventListener("keydown", key, true);
}

/** Déclare un élément comme cible de dépôt pour un type de contenu. */
export function useDropTarget<T extends HTMLElement>(kind: Payload["kind"], onDrop: (p: Payload) => void, enabled = true): RefObject<T | null> {
  const ref = useRef<T | null>(null);
  const handler = useRef(onDrop);
  handler.current = onDrop;
  useEffect(() => {
    const el = ref.current;
    if (!el || !enabled) return;
    targets.set(el, { kind, onDrop: (p) => handler.current(p) });
    return () => {
      targets.delete(el);
    };
  }, [kind, enabled]);
  return ref;
}
