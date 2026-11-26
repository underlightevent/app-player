import React from "react";
import ReactDOM from "react-dom/client";
import "./styles/tokens.css";
import "./styles/app.css";
import "./i18n";
import App from "./App";
import { logMessage } from "./core/api";
import { installBrowserShortcutBlocker } from "./core/shortcuts";
import { applyTheme, isThemeSetting, resolveTheme, systemPrefersDark } from "./core/theme";

// Évite un éclair blanc : on applique tout de suite le dernier thème connu.
// La valeur de référence reste celle de la base, chargée juste après.
try {
  const cached = localStorage.getItem("theme");
  if (isThemeSetting(cached)) applyTheme(resolveTheme(cached, systemPrefersDark()));
} catch {
  /* stockage indisponible : thème clair par défaut */
}

installBrowserShortcutBlocker();

window.addEventListener("error", (e) => logMessage("error", `Erreur interface : ${e.message}`));
window.addEventListener("unhandledrejection", (e) =>
  logMessage("error", `Promesse rejetée : ${String(e.reason)}`),
);

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
