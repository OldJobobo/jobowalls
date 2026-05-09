import { FolderOpen } from "lucide-react";

type Props = {
  message: string;
  onFolderPrompt: () => void;
};

export default function EmptyState({ message, onFolderPrompt }: Props) {
  return (
    <main className="empty-state">
      <FolderOpen size={38} />
      <p>{message}</p>
      <button type="button" onClick={onFolderPrompt}>
        Open Folder
      </button>
    </main>
  );
}
