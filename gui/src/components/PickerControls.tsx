import {
  BookmarkPlus,
  Check,
  FolderOpen,
  Monitor,
  Pause,
  Play,
  Plus,
  RefreshCw,
  Shuffle,
  Trash2,
  X,
} from "lucide-react";
import type { CollectionSummary, PreviewQuality, ThemeCollectionSummary } from "../lib/types";

type Props = {
  folder: string | null;
  sourceLabel?: string | null;
  planBackend?: string;
  monitor: string;
  monitorNames: string[];
  count: number;
  collections: CollectionSummary[];
  themeCollections: ThemeCollectionSummary[];
  activeCollectionId: string | null;
  selectedPath?: string | null;
  canRemoveSelected: boolean;
  previewQuality: PreviewQuality;
  livePreview: boolean;
  onApply: () => void;
  onShuffle: () => void;
  onRescan: () => void;
  onFolderPrompt: () => void;
  onCollectionChange: (id: string | null) => void;
  onCreateCollection: () => void;
  onAddToCollection: () => void;
  onRemoveFromCollection: () => void;
  onMonitorChange: (monitor: string) => void;
  onPreviewQualityChange: (quality: PreviewQuality) => void;
  onLivePreviewChange: (enabled: boolean) => void;
  onClose: () => void;
};

export default function PickerControls({
  folder,
  sourceLabel,
  planBackend,
  monitor,
  monitorNames,
  count,
  collections,
  themeCollections,
  activeCollectionId,
  selectedPath,
  canRemoveSelected,
  previewQuality,
  livePreview,
  onApply,
  onShuffle,
  onRescan,
  onFolderPrompt,
  onCollectionChange,
  onCreateCollection,
  onAddToCollection,
  onRemoveFromCollection,
  onMonitorChange,
  onPreviewQualityChange,
  onLivePreviewChange,
  onClose,
}: Props) {
  const monitorOptions = uniqueOptions(["all", monitor, ...monitorNames]);

  return (
    <header className="picker-controls">
      <div className="folder-summary">
        <span className="folder-label" title={folder ?? undefined}>
          {sourceLabel ?? folderLabel(folder)}
        </span>
        <span className="folder-count">{countLabel(count)}</span>
      </div>

      <label className="collection-control" title="Wallpaper collection">
        <select
          value={activeCollectionId ?? "__folder__"}
          aria-label="Wallpaper collection"
          onChange={(event) => {
            const value = event.target.value;
            onCollectionChange(value === "__folder__" ? null : value);
          }}
        >
          <option value="__folder__">Folder</option>
          {collections.length > 0 && (
            <optgroup label="Collections">
              {collections.map((collection) => (
                <option key={collection.id} value={collection.id}>
                  {collection.name}
                </option>
              ))}
            </optgroup>
          )}
          {themeCollections.length > 0 && (
            <optgroup label="Themes">
              {themeCollections.map((collection) => (
                <option key={collection.id} value={collection.id}>
                  Theme: {collection.name}
                </option>
              ))}
            </optgroup>
          )}
        </select>
      </label>

      <div className="plan-summary">
        <span>{planBackend ?? "auto"}</span>
      </div>

      <label className="monitor-control" title="Target monitor">
        <Monitor size={14} />
        <select value={monitor} onChange={(event) => onMonitorChange(event.target.value)}>
          {monitorOptions.map((option) => (
            <option key={option} value={option}>
              {option}
            </option>
          ))}
        </select>
      </label>

      <label
        className={livePreview ? "quality-control" : "quality-control disabled"}
        title={livePreview ? "Live preview quality" : "Enable live previews to use quality presets"}
      >
        <span>Quality</span>
        <select
          value={previewQuality}
          disabled={!livePreview}
          aria-label="Live preview quality"
          onChange={(event) => onPreviewQualityChange(event.target.value as PreviewQuality)}
        >
          <option value="fast">Fast</option>
          <option value="balanced">Balanced</option>
          <option value="pretty">Pretty</option>
        </select>
      </label>

      <div className="control-buttons">
        <button type="button" onClick={onFolderPrompt} title="Open folder" aria-label="Open folder">
          <FolderOpen size={17} />
        </button>
        <button type="button" onClick={onCreateCollection} title="Create collection" aria-label="Create collection">
          <Plus size={17} />
        </button>
        <button
          type="button"
          onClick={onAddToCollection}
          title="Add selected wallpaper to collection"
          aria-label="Add selected wallpaper to collection"
          disabled={!selectedPath}
        >
          <BookmarkPlus size={17} />
        </button>
        <button
          type="button"
          onClick={onRemoveFromCollection}
          title="Remove selected wallpaper from collection"
          aria-label="Remove selected wallpaper from collection"
          disabled={!selectedPath || !canRemoveSelected}
        >
          <Trash2 size={17} />
        </button>
        <button
          type="button"
          onClick={onRescan}
          title="Rescan"
          aria-label="Rescan"
          disabled={!folder && !activeCollectionId}
        >
          <RefreshCw size={17} />
        </button>
        <button type="button" onClick={onShuffle} title="Shuffle" aria-label="Shuffle" disabled={count < 2}>
          <Shuffle size={17} />
        </button>
        <button
          type="button"
          className={livePreview ? "live-toggle enabled" : "live-toggle"}
          onClick={() => onLivePreviewChange(!livePreview)}
          title={livePreview ? "Disable live previews" : "Enable live previews"}
          aria-label={livePreview ? "Disable live previews" : "Enable live previews"}
          aria-pressed={livePreview}
        >
          {livePreview ? <Pause size={17} /> : <Play size={17} />}
        </button>
        <button type="button" className="apply-button" onClick={onApply} title="Apply" disabled={count === 0}>
          <Check size={18} />
          Apply
        </button>
        <button type="button" onClick={onClose} title="Close" aria-label="Close">
          <X size={17} />
        </button>
      </div>
    </header>
  );
}

function folderLabel(folder: string | null) {
  if (!folder) {
    return "No folder";
  }

  const parts = folder.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? folder;
}

function countLabel(count: number) {
  return `${count} wallpaper${count === 1 ? "" : "s"}`;
}

function uniqueOptions(options: string[]) {
  return options
    .map((option) => option.trim())
    .filter(Boolean)
    .filter((option, index, all) => all.indexOf(option) === index);
}
