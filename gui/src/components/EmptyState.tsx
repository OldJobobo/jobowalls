import { FolderOpen } from "lucide-react";

type Props = {
  message: string;
  onFolderPrompt: () => void;
  loading?: boolean;
  showFolderButton?: boolean;
};

export default function EmptyState({
  message,
  onFolderPrompt,
  loading,
  showFolderButton = true,
}: Props) {
  return (
    <main className="empty-state">
      {loading ? <span className="loading-spinner" aria-hidden="true" /> : <FolderOpen size={38} />}
      <p>{message}</p>
      {showFolderButton && (
        <button type="button" onClick={onFolderPrompt}>
          Open Folder
        </button>
      )}
    </main>
  );
}
