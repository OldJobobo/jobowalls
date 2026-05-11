import { memo, useEffect, useState } from "react";
import { ImageIcon, Video } from "lucide-react";
import {
  getLivePreviewDataSource,
  getLivePreviewSource,
  getMediaDataSource,
  getMediaSource,
  getThumbnailDataSource,
  getThumbnailSource,
} from "../lib/invoke";
import type { MediaSource, PreviewQuality, WallpaperItem } from "../lib/types";

type Props = {
  item: WallpaperItem;
  className?: string;
  alt?: string;
  decorative?: boolean;
  playLive?: boolean;
  mode?: "preview" | "thumbnail";
  quality?: PreviewQuality;
};

const sourceCache = new Map<string, Promise<MediaSource>>();
const fallbackCache = new Map<string, Promise<MediaSource>>();
let assetPreviewFailed = window.localStorage.getItem("jobowalls:assetPreviewFailed") === "1";
const PREVIEW_UPGRADE_DELAY_MS = 220;

function MediaPreview({
  item,
  className,
  alt,
  decorative,
  playLive,
  mode = "preview",
  quality = "balanced",
}: Props) {
  const [src, setSrc] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);
  const [fallbackTried, setFallbackTried] = useState(false);

  useEffect(() => {
    let cancelled = false;
    setFailed(false);
    setFallbackTried(false);

    async function load() {
      try {
        if (mode === "thumbnail") {
          const result = await cachedPreferredThumbnailSource(item.path);
          if (!cancelled) {
            setSrc((current) => result.src ?? current);
            setFailed(!result.src);
          }
          return;
        }

        const thumbnail = await cachedPreferredThumbnailSource(item.path);
        if (!cancelled) {
          setSrc((current) => current ?? thumbnail.src);
          setFailed(!thumbnail.src);
        }

        await delay(PREVIEW_UPGRADE_DELAY_MS);
        if (cancelled) {
          return;
        }

        if (item.kind === "live" && playLive) {
          const animated = await cachedPreferredLivePreviewSource(item.path, quality);
          if (!cancelled) {
            setSrc((current) => animated.src ?? current);
            setFailed(!animated.src);
          }
          return;
        }

        const result = await cachedPreferredMediaSource(item.path);
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
  }, [item.kind, item.path, mode, playLive, quality]);

  async function loadFallback() {
    assetPreviewFailed = true;
    window.localStorage.setItem("jobowalls:assetPreviewFailed", "1");
    if (fallbackTried) {
      setFailed(true);
      return;
    }

    setFallbackTried(true);
    try {
      const result =
        mode === "thumbnail"
          ? await cachedThumbnailDataSource(item.path)
          : item.kind === "live" && playLive
          ? await cachedLivePreviewDataSource(item.path, quality)
          : await cachedMediaDataSource(item.path);
      setSrc(result.src);
      setFailed(!result.src);
    } catch {
      setFailed(true);
    }
  }

  if (src) {
    if (item.kind === "live" && playLive && mode === "preview" && isVideoSource(src)) {
      return (
        <video
          className={className}
          src={src}
          aria-label={decorative ? undefined : alt ?? item.name}
          autoPlay
          loop
          muted
          playsInline
          onError={() => void loadFallback()}
        />
      );
    }

    return (
      <img
        className={className}
        src={src}
        alt={decorative ? "" : alt ?? item.name}
        onError={() => void loadFallback()}
      />
    );
  }

  return (
    <span className={["media-placeholder", className, failed ? "failed" : ""].filter(Boolean).join(" ")}>
      {item.kind === "live" ? <Video size={24} /> : <ImageIcon size={24} />}
    </span>
  );
}

export default memo(MediaPreview);

function cachedPreferredMediaSource(path: string) {
  return assetPreviewFailed ? cachedMediaDataSource(path) : cachedMediaSource(path);
}

function cachedPreferredThumbnailSource(path: string) {
  return assetPreviewFailed ? cachedThumbnailDataSource(path) : cachedThumbnailSource(path);
}

function cachedPreferredLivePreviewSource(path: string, quality: PreviewQuality) {
  return assetPreviewFailed
    ? cachedLivePreviewDataSource(path, quality)
    : cachedLivePreviewSource(path, quality);
}

function cachedMediaSource(path: string) {
  return cachedSource(`poster:${path}`, () => getMediaSource(path));
}

function cachedThumbnailSource(path: string) {
  return cachedSource(`thumb:${path}`, () => getThumbnailSource(path));
}

function cachedLivePreviewSource(path: string, quality: PreviewQuality) {
  return getLivePreviewSource(path, quality);
}

function cachedMediaDataSource(path: string) {
  return cachedFallback(`poster:${path}`, () => getMediaDataSource(path));
}

function cachedThumbnailDataSource(path: string) {
  return cachedFallback(`thumb:${path}`, () => getThumbnailDataSource(path));
}

function cachedLivePreviewDataSource(path: string, quality: PreviewQuality) {
  return getLivePreviewDataSource(path, quality);
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

function cachedFallback(key: string, load: () => Promise<MediaSource>) {
  const cached = fallbackCache.get(key);
  if (cached) {
    return cached;
  }

  const promise = load().catch((error) => {
    fallbackCache.delete(key);
    throw error;
  });
  fallbackCache.set(key, promise);
  return promise;
}

function delay(ms: number) {
  return new Promise((resolve) => window.setTimeout(resolve, ms));
}

function isVideoSource(src: string) {
  return src.includes(".mp4") || src.startsWith("data:video/");
}
