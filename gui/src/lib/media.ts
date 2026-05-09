import type { WallpaperItem } from "./types";

export function mediaSrc(path: string) {
  return path;
}

export function shuffledIndex(items: WallpaperItem[], currentIndex: number) {
  if (items.length <= 1) {
    return currentIndex;
  }

  let next = Math.floor(Math.random() * items.length);
  if (next === currentIndex) {
    next = (next + 1) % items.length;
  }
  return next;
}
