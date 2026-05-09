import { ImageIcon, Video } from "lucide-react";
import type { WallpaperItem } from "../lib/types";
import MediaPreview from "./MediaPreview";

type Props = {
  items: WallpaperItem[];
  selectedIndex: number;
  activePath?: string;
  applyingPath?: string | null;
  onSelect: (index: number) => void;
  onApply: (index: number) => void;
};

export default function FilmRoll({
  items,
  selectedIndex,
  activePath,
  applyingPath,
  onSelect,
  onApply,
}: Props) {
  const visible = visibleItems(items, selectedIndex);

  return (
    <div className="film-roll" aria-label="Wallpaper film roll">
      {visible.map(({ item, index, distance }) => (
        <button
          key={item.path}
          className={[
            "film-item",
            distance === 0 ? "selected" : "",
            Math.abs(distance) === 1 ? "near" : "",
            Math.abs(distance) > 1 ? "far" : "",
            activePath === item.path ? "active" : "",
            applyingPath === item.path ? "applying" : "",
          ]
            .filter(Boolean)
            .join(" ")}
          style={
            {
              "--offset": distance,
              zIndex: 10 - Math.abs(distance),
            } as React.CSSProperties
          }
          onClick={() => onSelect(index)}
          onDoubleClick={() => onApply(index)}
          title={item.name}
        >
          <span className="thumb-frame">
            <MediaPreview item={item} decorative />
          </span>
          <span className="thumb-meta">
            {item.kind === "live" ? <Video size={12} /> : <ImageIcon size={12} />}
            <span>{item.name}</span>
          </span>
        </button>
      ))}
    </div>
  );
}

function visibleItems(items: WallpaperItem[], selectedIndex: number) {
  if (items.length === 0) {
    return [];
  }

  const radius = Math.min(4, Math.floor(items.length / 2));
  const result: Array<{ item: WallpaperItem; index: number; distance: number }> = [];
  const seen = new Set<number>();

  for (let distance = -radius; distance <= radius; distance += 1) {
    const index = wrapIndex(selectedIndex + distance, items.length);
    if (seen.has(index)) {
      continue;
    }
    seen.add(index);
    result.push({ item: items[index], index, distance });
  }

  return result;
}

function wrapIndex(index: number, length: number) {
  return ((index % length) + length) % length;
}
