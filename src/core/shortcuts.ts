/**
 * Raccourcis propres au navigateur intégré, désactivés car ils entreraient
 * en conflit avec ceux de l'application (cahier des charges, « Raccourcis »).
 */
export function isBrowserShortcut(e: Pick<KeyboardEvent, "key" | "ctrlKey" | "metaKey" | "altKey">): boolean {
  const mod = e.ctrlKey || e.metaKey;
  const key = e.key.toLowerCase();
  if (key === "f5" || key === "f7" || key === "browserrefresh") return true; // rechargement, navigation au curseur
  if (mod && (key === "r" || key === "p" || key === "u" || key === "s" || key === "g" || key === "j")) return true; // recharger, imprimer, source, enregistrer, chercher, téléchargements
  if (mod && (key === "+" || key === "=" || key === "-" || key === "0")) return true; // zoom
  if (e.altKey && (key === "arrowleft" || key === "arrowright")) return true; // précédent / suivant
  return false;
}

export function installBrowserShortcutBlocker() {
  window.addEventListener(
    "keydown",
    (e) => {
      if (isBrowserShortcut(e)) e.preventDefault();
    },
    { capture: true },
  );
  // Ctrl + molette = zoom de page
  window.addEventListener(
    "wheel",
    (e) => {
      if (e.ctrlKey) e.preventDefault();
    },
    { passive: false },
  );
  if (!import.meta.env.DEV) {
    window.addEventListener("contextmenu", (e) => e.preventDefault());
  }
}
