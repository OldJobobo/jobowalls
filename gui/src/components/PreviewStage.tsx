import { ImageIcon, Video } from "lucide-react";
import type { PreviewQuality, WallpaperItem } from "../lib/types";
import MediaPreview from "./MediaPreview";

type Props = {
  item: WallpaperItem | null;
  activePath?: string;
  applying: boolean;
  previewQuality: PreviewQuality;
  livePreview: boolean;
};

export default function PreviewStage({
  item,
  activePath,
  applying,
  previewQuality,
  livePreview,
}: Props) {
  if (!item) {
    return (
      <section className="preview-stage empty-preview">
        <ImageIcon size={34} />
      </section>
    );
  }

  const active = activePath === item.path;

  return (
    <section className="preview-stage">
      <div className="preview-backdrop" aria-hidden="true">
        <MediaPreview item={item} decorative mode="thumbnail" />
      </div>
      <div className="preview-media">
        <MediaPreview item={item} alt={item.name} playLive={livePreview} quality={previewQuality} />
      </div>
      <div className="preview-caption">
        <span className="preview-name">{item.name}</span>
        <span className={`kind-pill ${item.kind}`}>
          {item.kind === "live" ? <Video size={13} /> : <ImageIcon size={13} />}
          {item.kind}
        </span>
        {active && <span className="status-pill">active</span>}
        {applying && <span className="status-pill applying">applying</span>}
      </div>
    </section>
  );
}
