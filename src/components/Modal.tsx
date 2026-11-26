import { useEffect, useRef, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { X } from "lucide-react";

interface Props {
  title: string;
  onClose: () => void;
  children: ReactNode;
  footer?: ReactNode;
  wide?: boolean;
}

/** Fenêtre de dialogue dans l'application : Échap ferme, le focus reste à l'intérieur. */
export function Modal({ title, onClose, children, footer, wide }: Props) {
  const { t } = useTranslation();
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const prev = document.activeElement as HTMLElement | null;
    const first = ref.current?.querySelector<HTMLElement>("input, select, textarea, button:not(.modal-close)");
    first?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        onClose();
      }
      if (e.key === "Tab" && ref.current) {
        const items = [...ref.current.querySelectorAll<HTMLElement>("input, select, textarea, button, [tabindex='0']")].filter((el) => !el.hasAttribute("disabled"));
        if (items.length === 0) return;
        const firstEl = items[0];
        const lastEl = items[items.length - 1];
        if (e.shiftKey && document.activeElement === firstEl) {
          e.preventDefault();
          lastEl.focus();
        } else if (!e.shiftKey && document.activeElement === lastEl) {
          e.preventDefault();
          firstEl.focus();
        }
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      prev?.focus?.();
    };
  }, [onClose]);
  return (
    <>
      <div className="menu-backdrop dim" onClick={onClose} />
      <div ref={ref} className={`modal ${wide ? "wide" : ""}`} role="dialog" aria-modal aria-label={title}>
        <div className="modal-head">
          <h2>{title}</h2>
          <button className="icon-btn modal-close" onClick={onClose} aria-label={t("common.close")}>
            <X size={18} aria-hidden />
          </button>
        </div>
        <div className="modal-body">{children}</div>
        {footer && <div className="modal-foot">{footer}</div>}
      </div>
    </>
  );
}
