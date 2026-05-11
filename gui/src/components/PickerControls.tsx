import { Check, FolderOpen, RefreshCw, Shuffle, X } from "lucide-react";
import type { PreviewQuality } from "../lib/types";

type Props = {
  folder: string | null;
  planBackend?: string;
  monitor: string;
  count: number;
  previewQuality: PreviewQuality;
  onApply: () => void;
  onShuffle: () => void;
  onRescan: () => void;
  onFolderPrompt: () => void;
  onPreviewQualityChange: (quality: PreviewQuality) => void;
  onClose: () => void;
};

export default function PickerControls({
  folder,
  planBackend,
  monitor,
  count,
  previewQuality,
  onApply,
  onShuffle,
  onRescan,
  onFolderPrompt,
  onPreviewQualityChange,
  onClose,
}: Props) {
  return (
    <header className="picker-controls">
      <div className="folder-summary">
        <span className="folder-label" title={folder ?? undefined}>
          {folderLabel(folder)}
        </span>
        <span className="folder-count">{countLabel(count)}</span>
      </div>

      <div className="plan-summary">
        <span>{planBackend ?? "auto"}</span>
        <span>{monitor}</span>
      </div>

      <label className="quality-control" title="Live preview quality">
        <span>Preview</span>
        <select
          value={previewQuality}
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
        <button type="button" onClick={onRescan} title="Rescan" aria-label="Rescan" disabled={!folder}>
          <RefreshCw size={17} />
        </button>
        <button type="button" onClick={onShuffle} title="Shuffle" aria-label="Shuffle" disabled={count < 2}>
          <Shuffle size={17} />
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
