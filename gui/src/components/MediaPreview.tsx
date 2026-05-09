import { useEffect, useState } from "react";
import { ImageIcon, Video } from "lucide-react";
import { getLivePreviewSource, getMediaSource } from "../lib/invoke";
import type { MediaSource, WallpaperItem } from "../lib/types";

type Props = {
  item: WallpaperItem;
  className?: string;
  alt?: string;
  decorative?: boolean;
  playLive?: boolean;
};

const sourceCache = new Map<string, Promise<MediaSource>>();

export default function MediaPreview({ item, className, alt, decorative, playLive }: Props) {
  const [src, setSrc] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let cancelled = false;
    setFailed(false);

    async function load() {
      try {
        if (item.kind === "live" && playLive) {
          void cachedMediaSource(item.path).then((poster) => {
            if (!cancelled) {
              setSrc((current) => current ?? poster.src);
              setFailed(!poster.src);
            }
          });

          const animated = await cachedLivePreviewSource(item.path);
          if (!cancelled) {
            setSrc((current) => animated.src ?? current);
            setFailed(!animated.src);
          }
          return;
        }

        const result = await cachedMediaSource(item.path);
        if (!cancelled) {
          setSrc((current) => result.src ?? current);
          setFailed(!result.src);
        }
      } catch {
        if (!cancelled) {
          setFailed(true);
        }
      }
    }

    void load();
    return () => {
      cancelled = true;
    };
  }, [item.kind, item.path, playLive]);

  if (src) {
    return <img className={className} src={src} alt={decorative ? "" : alt ?? item.name} />;
  }

  return (
    <span className={["media-placeholder", className, failed ? "failed" : ""].filter(Boolean).join(" ")}>
      {item.kind === "live" ? <Video size={24} /> : <ImageIcon size={24} />}
    </span>
  );
}

function cachedMediaSource(path: string) {
  return cachedSource(`poster:${path}`, () => getMediaSource(path));
}

function cachedLivePreviewSource(path: string) {
  return cachedSource(`live:${path}`, () => getLivePreviewSource(path));
}

function cachedSource(key: string, load: () => Promise<MediaSource>) {
  const cached = sourceCache.get(key);
  if (cached) {
    return cached;
  }

  const promise = load().catch((error) => {
    sourceCache.delete(key);
    throw error;
  });
  sourceCache.set(key, promise);
  return promise;
}
