import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import type {
  JobowallsStatus,
  MediaSource,
  SetPlanPreview,
  StartupFolder,
  WallpaperItem,
} from "./types";

export function resolveStartupFolder(inputPath?: string | null) {
  return invoke<StartupFolder>("resolve_startup_folder", {
    inputPath: inputPath ?? null,
  });
}

export function scanFolder(path: string) {
  return invoke<WallpaperItem[]>("scan_folder", { path });
}

export function getStatus() {
  return invoke<JobowallsStatus>("get_status");
}

export function previewPlan(path: string, monitor?: string | null) {
  return invoke<SetPlanPreview>("preview_plan", {
    path,
    monitor: monitor ?? null,
  });
}

export function applyWallpaper(path: string, monitor?: string | null) {
  return invoke<void>("apply_wallpaper", {
    path,
    monitor: monitor ?? null,
  });
}

export function getMediaSource(path: string) {
  return invoke<MediaSource>("get_media_source", { path }).then(withAssetUrl);
}

export function getMediaDataSource(path: string) {
  return invoke<MediaSource>("get_media_data_source", { path });
}

export function getLivePreviewSource(path: string) {
  return invoke<MediaSource>("get_live_preview_source", { path }).then(withAssetUrl);
}

export function getLivePreviewDataSource(path: string) {
  return invoke<MediaSource>("get_live_preview_data_source", { path });
}

export function warmLivePreview(path: string) {
  return invoke<void>("warm_live_preview", { path });
}

export function saveLastFolder(path: string) {
  return invoke<void>("save_last_folder", { path });
}

export function closePicker() {
  return invoke<void>("close_picker");
}

function withAssetUrl(source: MediaSource): MediaSource {
  if (!source.src) {
    return source;
  }

  return {
    ...source,
    src: convertFileSrc(source.src),
  };
}
