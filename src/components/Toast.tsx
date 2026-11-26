import { useCallback, useMemo, useRef, useState } from "react";

/** Notification discrète en bas de l'écran (un bouton « Annuler » s'y ajoutera en phase 1). */
export function useToast() {
  const [message, setMessage] = useState<string | null>(null);
  const timer = useRef<number | undefined>(undefined);
  const show = useCallback((text: string) => {
    setMessage(text);
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => setMessage(null), 3000);
  }, []);
  return useMemo(() => ({ message, show }), [message, show]);
}

export function Toast({ message }: { message: string | null }) {
  return (
    <div aria-live="polite" className={message ? "toast" : "visually-hidden"}>
      {message}
    </div>
  );
}
