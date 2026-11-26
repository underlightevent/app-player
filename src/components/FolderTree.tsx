import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { ChevronDown, ChevronRight, Folder, FolderOpen, FolderPlus, HardDrive, Pencil, SquareArrowOutUpRight, Unplug } from "lucide-react";
import type { RootInfo } from "../core/library";
import { foldersList, type DirInfo } from "../core/folders";
import { useDropTarget } from "../core/dnd";

export interface FolderTarget {
  volumeId: number;
  relPath: string;
  name: string;
}

interface Props {
  roots: RootInfo[];
  /** Dossier filtré actuellement dans la bibliothèque. */
  current: FolderTarget | null;
  onOpen: (f: FolderTarget) => void;
  onDropTracks: (ids: number[], f: FolderTarget) => void;
  onCreate: (parent: FolderTarget) => void;
  onRename: (f: FolderTarget) => void;
  onReveal: (f: FolderTarget) => void;
  /** Change après chaque opération sur les dossiers : relit l'arborescence. */
  revision: number;
}

function Node(props: Props & { dir: FolderTarget; count: number; hasChildren: boolean; depth: number; online: boolean; isRoot?: boolean }) {
  const { t, i18n } = useTranslation();
  const [open, setOpen] = useState(false);
  const [children, setChildren] = useState<DirInfo[] | null>(null);
  const nf = new Intl.NumberFormat(i18n.language);
  const dropRef = useDropTarget<HTMLDivElement>(
    "tracks",
    (p) => {
      if (p.kind === "tracks" && p.ids.length) props.onDropTracks(p.ids, props.dir);
    },
    props.online,
  );

  const load = useCallback(async () => {
    setChildren(await foldersList(props.dir.volumeId, props.dir.relPath).catch(() => []));
  }, [props.dir.volumeId, props.dir.relPath]);

  useEffect(() => {
    if (open) void load();
  }, [open, load, props.revision]);

  const active = props.current?.volumeId === props.dir.volumeId && props.current.relPath === props.dir.relPath;
  const Icon = props.isRoot ? (props.online ? HardDrive : Unplug) : open ? FolderOpen : Folder;
  return (
    <li role="treeitem" aria-expanded={props.hasChildren ? open : undefined} aria-selected={active}>
      <div
        ref={dropRef}
        className={`tree-row ${active ? "active" : ""} ${props.online ? "" : "offline"}`}
        style={{ paddingLeft: 4 + props.depth * 12 }}
      >
        <button className="tree-toggle" onClick={() => setOpen((o) => !o)} aria-label={open ? t("folders.collapse") : t("folders.expand")} disabled={!props.hasChildren}>
          {props.hasChildren ? open ? <ChevronDown size={14} aria-hidden /> : <ChevronRight size={14} aria-hidden /> : null}
        </button>
        <button className="tree-label" onClick={() => props.onOpen(props.dir)} title={props.dir.relPath}>
          <Icon size={16} aria-hidden />
          <span className="tree-name">{props.dir.name}</span>
          <span className="tree-count tabular">{nf.format(props.count)}</span>
        </button>
        {props.online && (
          <span className="tree-actions">
            <button className="icon-btn small" onClick={() => props.onCreate(props.dir)} aria-label={t("folders.newSub")} title={t("folders.newSub")}>
              <FolderPlus size={14} aria-hidden />
            </button>
            {!props.isRoot && (
              <button className="icon-btn small" onClick={() => props.onRename(props.dir)} aria-label={t("folders.rename")} title={t("folders.rename")}>
                <Pencil size={14} aria-hidden />
              </button>
            )}
            <button className="icon-btn small" onClick={() => props.onReveal(props.dir)} aria-label={t("actions.reveal")} title={t("actions.reveal")}>
              <SquareArrowOutUpRight size={14} aria-hidden />
            </button>
          </span>
        )}
      </div>
      {open && children && children.length > 0 && (
        <ul role="group">
          {children.map((c) => (
            <Node
              key={c.relPath}
              {...props}
              isRoot={false}
              dir={{ volumeId: c.volumeId, relPath: c.relPath, name: c.name }}
              count={c.trackCount}
              hasChildren={c.hasChildren}
              depth={props.depth + 1}
            />
          ))}
        </ul>
      )}
    </li>
  );
}

/** Arborescence des dossiers de la bibliothèque (barre latérale). */
export function FolderTree(props: Props) {
  const { t } = useTranslation();
  const roots = props.roots.filter((r) => r.kind === "library");
  if (roots.length === 0) return null;
  return (
    <ul className="folder-tree" role="tree" aria-label={t("nav.folders")}>
      {roots.map((r) => (
        <Node
          key={r.id}
          {...props}
          isRoot
          online={r.online}
          dir={{ volumeId: r.volumeId, relPath: r.relPath, name: r.relPath.split("/").pop() || r.volumeLabel }}
          count={r.trackCount}
          hasChildren
          depth={0}
        />
      ))}
    </ul>
  );
}
