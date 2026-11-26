import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Loader2, X } from "lucide-react";
import { onEvent, taskCancel, tasksList, type TaskInfo } from "../core/library";

/** Suit les tâches de fond envoyées par le cœur. */
export function useTasks() {
  const [tasks, setTasks] = useState<Map<number, TaskInfo>>(new Map());
  useEffect(() => {
    void tasksList().then((list) => setTasks(new Map(list.map((t) => [t.id, t]))));
    return onEvent<TaskInfo>("tasks:update", (t) =>
      setTasks((prev) => {
        const next = new Map(prev);
        next.set(t.id, t);
        return next;
      }),
    );
  }, []);
  const list = [...tasks.values()].sort((a, b) => b.id - a.id);
  return { list, running: list.filter((t) => t.state === "running") };
}

function eta(t: TaskInfo): number | null {
  if (t.state !== "running" || t.done < 5 || t.total <= t.done) return null;
  const elapsed = Date.now() - t.started_ms;
  return ((t.total - t.done) * elapsed) / t.done;
}

export function TasksPanel({ list, onClose }: { list: TaskInfo[]; onClose: () => void }) {
  const { t, i18n } = useTranslation();
  const nf = new Intl.NumberFormat(i18n.language);
  return (
    <>
      <div className="menu-backdrop" onClick={onClose} />
      <div className="popover tasks-panel" role="dialog" aria-label={t("tasks.title")}>
        <div className="popover-title">{t("tasks.title")}</div>
        {list.length === 0 && <p className="muted">{t("tasks.none")}</p>}
        {list.slice(0, 8).map((task) => {
          const pct = task.total > 0 ? Math.min(100, (task.done / task.total) * 100) : null;
          const remaining = eta(task);
          return (
            <div key={task.id} className="task">
              <div className="task-head">
                {task.state === "running" && <Loader2 size={14} className="spin" aria-hidden />}
                <span className="task-label">{t(task.label)}</span>
                <span className="task-state">{t(`tasks.state.${task.state}`)}</span>
                {task.state === "running" && (
                  <button className="icon-btn small" onClick={() => void taskCancel(task.id)} aria-label={t("tasks.cancel")} title={t("tasks.cancel")}>
                    <X size={14} aria-hidden />
                  </button>
                )}
              </div>
              {task.detail && <div className="task-detail" title={task.detail}>{task.detail}</div>}
              {task.state === "running" && (
                <>
                  <div className="progress" role="progressbar" aria-valuemin={0} aria-valuemax={task.total || undefined} aria-valuenow={task.done}>
                    <div className={`progress-bar ${pct == null ? "indeterminate" : ""}`} style={pct == null ? undefined : { width: `${pct}%` }} />
                  </div>
                  <div className="task-detail tabular">
                    {task.total > 0 ? `${nf.format(task.done)} / ${nf.format(task.total)}` : ""}
                    {remaining != null && ` · ${t("tasks.remaining", { minutes: Math.max(1, Math.round(remaining / 60000)) })}`}
                  </div>
                </>
              )}
              {task.error && <div className="task-detail text-error">{task.error}</div>}
            </div>
          );
        })}
      </div>
    </>
  );
}
