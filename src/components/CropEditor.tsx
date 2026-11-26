import { useEffect, useRef, useState, type MouseEvent as ReactMouseEvent } from "react";
import { useTranslation } from "react-i18next";
import { RotateCw } from "lucide-react";
import { Modal } from "./Modal";

/** Recadrage choisi : proportions (0 à 1) de l'image après rotation. */
export interface CropEdit {
  x: number;
  y: number;
  w: number;
  h: number;
  rotate: number;
}

const BOX = 420;

/** Image tournée de `quarter` quarts de tour, rendue dans un canevas. */
function rotated(img: HTMLImageElement, quarter: number): HTMLCanvasElement {
  const c = document.createElement("canvas");
  const swap = quarter % 2 === 1;
  c.width = swap ? img.naturalHeight : img.naturalWidth;
  c.height = swap ? img.naturalWidth : img.naturalHeight;
  const ctx = c.getContext("2d")!;
  ctx.translate(c.width / 2, c.height / 2);
  ctx.rotate((quarter * Math.PI) / 2);
  ctx.drawImage(img, -img.naturalWidth / 2, -img.naturalHeight / 2);
  return c;
}

/** Éditeur de pochette : recadrage carré, rotation, comparaison avant / après. */
export function CropEditor({ src, onCancel, onConfirm }: { src: string; onCancel: () => void; onConfirm: (e: CropEdit) => void }) {
  const { t } = useTranslation();
  const [img, setImg] = useState<HTMLImageElement | null>(null);
  const [quarter, setQuarter] = useState(0);
  const [view, setView] = useState<{ url: string; w: number; h: number } | null>(null);
  // Sélection carrée en pixels de l'image tournée.
  const [sel, setSel] = useState({ x: 0, y: 0, s: 100 });
  const [compare, setCompare] = useState(false);
  const previewRef = useRef<HTMLCanvasElement>(null);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);

  useEffect(() => {
    const i = new Image();
    i.onload = () => setImg(i);
    i.src = src;
  }, [src]);

  // Nouvelle rotation : image redessinée, sélection carrée la plus grande possible au centre.
  useEffect(() => {
    if (!img) return;
    const c = rotated(img, quarter);
    canvasRef.current = c;
    setView({ url: c.toDataURL("image/jpeg", 0.9), w: c.width, h: c.height });
    const s = Math.min(c.width, c.height);
    setSel({ x: (c.width - s) / 2, y: (c.height - s) / 2, s });
  }, [img, quarter]);

  // Aperçu du résultat.
  useEffect(() => {
    const out = previewRef.current;
    const c = canvasRef.current;
    if (!out || !c) return;
    const ctx = out.getContext("2d")!;
    ctx.clearRect(0, 0, out.width, out.height);
    ctx.drawImage(c, sel.x, sel.y, sel.s, sel.s, 0, 0, out.width, out.height);
  }, [sel, view, compare]);

  if (!view) return null;
  const scale = Math.min(BOX / view.w, BOX / view.h);
  const dw = view.w * scale;
  const dh = view.h * scale;

  const drag = (e: ReactMouseEvent, mode: "move" | "resize") => {
    e.preventDefault();
    e.stopPropagation();
    const start = { mx: e.clientX, my: e.clientY, ...sel };
    const move = (ev: MouseEvent) => {
      const dx = (ev.clientX - start.mx) / scale;
      const dy = (ev.clientY - start.my) / scale;
      if (mode === "move") {
        setSel({ s: start.s, x: Math.min(Math.max(0, start.x + dx), view.w - start.s), y: Math.min(Math.max(0, start.y + dy), view.h - start.s) });
      } else {
        const maxS = Math.min(view.w - start.x, view.h - start.y);
        setSel({ x: start.x, y: start.y, s: Math.min(Math.max(64, start.s + Math.max(dx, dy)), maxS) });
      }
    };
    const up = () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  };

  return (
    <Modal
      title={t("crop.title")}
      onClose={onCancel}
      wide
      footer={
        <>
          <span className="muted small tabular">
            {Math.round(sel.s)} × {Math.round(sel.s)} px{sel.s < 500 ? ` · ${t("crop.small")}` : ""}
          </span>
          <button className="btn" onClick={onCancel}>{t("common.cancel")}</button>
          <button
            className="btn btn-primary"
            onClick={() => onConfirm({ x: sel.x / view.w, y: sel.y / view.h, w: sel.s / view.w, h: sel.s / view.h, rotate: quarter })}
          >
            {t("crop.apply")}
          </button>
        </>
      }
    >
      <div className="crop-tools">
        <button className="btn" onClick={() => setQuarter((q) => (q + 1) % 4)}>
          <RotateCw size={16} aria-hidden /> {t("crop.rotate")}
        </button>
        <label className="small">
          <input type="checkbox" checked={compare} onChange={(e) => setCompare(e.target.checked)} /> {t("crop.compare")}
        </label>
      </div>
      <div className="crop-area">
        <div className="crop-stage" style={{ width: dw, height: dh, backgroundImage: `url(${view.url})` }}>
          <div
            className="crop-sel"
            style={{ left: sel.x * scale, top: sel.y * scale, width: sel.s * scale, height: sel.s * scale }}
            onMouseDown={(e) => drag(e, "move")}
            aria-label={t("crop.selection")}
          >
            <span className="crop-handle" onMouseDown={(e) => drag(e, "resize")} />
          </div>
        </div>
        {compare && (
          <div className="crop-compare">
            <div>
              <div className="small muted">{t("crop.before")}</div>
              <img src={src} alt="" />
            </div>
            <div>
              <div className="small muted">{t("crop.after")}</div>
              <canvas ref={previewRef} width={200} height={200} />
            </div>
          </div>
        )}
        {!compare && <canvas ref={previewRef} width={200} height={200} className="crop-result" />}
      </div>
      <p className="hint small">{t("crop.hint")}</p>
    </Modal>
  );
}
