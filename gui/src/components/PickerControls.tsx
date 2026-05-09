import { Check, FolderOpen, RefreshCw, Shuffle, X } from "lucide-react";

type Props = {
  folder: string | null;
  planBackend?: string;
  monitor: string;
  count: number;
  onApply: () => void;
  onShuffle: () => void;
  onRescan: () => void;
  onFolderPrompt: () => void;
  onClose: () => void;
};

export default function PickerControls({
  folder,
  planBackend,
  monitor,
  count,
  onApply,
  onShuffle,
  onRescan,
  onFolderPrompt,
  onClose,
}: Props) {
  return (
    <header className="picker-controls">
      <div className="folder-summary">
        <span className="folder-label">{folder ?? "No folder"}</span>
        <span className="folder-count">{count} wallpapers</span>
      </div>

      <div className="plan-summary">
        <span>{planBackend ?? "auto"}</span>
        <span>{monitor}</span>
      </div>

      <div className="control-buttons">
        <button type="button" onClick={onFolderPrompt} title="Open folder">
          <FolderOpen size={17} />
        </button>
        <button type="button" onClick={onRescan} title="Rescan">
          <RefreshCw size={17} />
        </button>
        <button type="button" onClick={onShuffle} title="Shuffle">
          <Shuffle size={17} />
        </button>
        <button type="button" className="apply-button" onClick={onApply} title="Apply">
          <Check size={18} />
          Apply
        </button>
        <button type="button" onClick={onClose} title="Close">
          <X size={17} />
        </button>
      </div>
    </header>
  );
}
