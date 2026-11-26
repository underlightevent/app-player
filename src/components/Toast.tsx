import { useCallback, useMemo, useRef, useState } from "react";

export interface ToastAction {
  label: string;
  run: () => void;
}

interface ToastState {
  message: string;
  action?: ToastAction;
  tone?: "error";
}

/** Notification discrète en bas de l'écran, avec un bouton (« Annuler ») si besoin. */
export function useToast() {
  const [toast, setToast] = useState<ToastState | null>(null);
  const timer = useRef<number | undefined>(undefined);
  const show = useCallback((message: string, action?: ToastAction, tone?: "error") => {
    setToast({ message, action, tone });
    window.clearTimeout(timer.current);
    // Plus longtemps quand il y a une action ou une erreur, pour laisser le temps de lire.
    timer.current = window.setTimeout(() => setToast(null), action || tone ? 8000 : 3000);
  }, []);
  const hide = useCallback(() => setToast(null), []);
  return useMemo(() => ({ toast, show, hide }), [toast, show, hide]);
}

export function Toast({ toast, onHide }: { toast: ToastState | null; onHide: () => void }) {
  return (
    <div aria-live="polite" className={toast ? `toast ${toast.tone === "error" ? "toast-error" : ""}` : "visually-hidden"}>
      {toast?.message}
      {toast?.action && (
        <button
          className="toast-action"
          onClick={() => {
            toast.action?.run();
            onHide();
          }}
        >
          {toast.action.label}
        </button>
      )}
    </div>
  );
}
