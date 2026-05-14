import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import type {
  CollectionDetail,
  CollectionSummary,
  JobowallsStatus,
  GuiConfig,
  GuiState,
  GuiStartupOptions,
  MediaSource,
  OmarchyThemeColors,
  PreviewQuality,
  SetPlanPreview,
  StartupFolder,
  ThemeCollectionAddTarget,
  ThemeCollectionDetail,
  ThemeCollectionImport,
  ThemeCollectionSummary,
  WallpaperItem,
} from "./types";

export function getStartupOptions() {
  return invoke<GuiStartupOptions>("get_startup_options");
}

export function resolveStartupFolder(inputPath?: string | null) {
  return invoke<StartupFolder>("resolve_startup_folder", {
    inputPath: inputPath ?? null,
  });
}

export function scanFolder(path: string) {
  return invoke<WallpaperItem[]>("scan_folder", { path });
}

export function getOmarchyThemeColors() {
  return invoke<OmarchyThemeColors | null>("get_omarchy_theme_colors");
}

export function getGuiConfig() {
  return invoke<GuiConfig>("get_gui_config");
}

export function getGuiState() {
  return invoke<GuiState>("get_gui_state");
}

export function getMonitorNames() {
  return invoke<string[]>("get_monitor_names");
}

export function getStatus() {
  return invoke<JobowallsStatus>("get_status");
}

export function adoptOmarchyBackground() {
  return invoke<{ adopted: boolean; message: string; wallpaper?: string | null }>("adopt_omarchy_background");
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

export function listCollections() {
  return invoke<CollectionSummary[]>("list_collections");
}

export function createCollection(name: string) {
  return invoke<CollectionSummary>("create_collection", { name });
}

export function getCollection(id: string) {
  return invoke<CollectionDetail>("get_collection", { id });
}

export function addToCollection(id: string, paths: string[]) {
  return invoke<CollectionDetail>("add_to_collection", { id, paths });
}

export function removeFromCollection(id: string, path: string) {
  return invoke<CollectionDetail>("remove_from_collection", { id, path });
}

export function deleteCollection(id: string) {
  return invoke<CollectionSummary>("delete_collection", { id });
}

export function listThemeCollections() {
  return invoke<ThemeCollectionSummary[]>("list_theme_collections");
}

export function getThemeCollection(id: string) {
  return invoke<ThemeCollectionDetail>("get_theme_collection", { id });
}

export function addToThemeCollection(id: string, path: string, target?: ThemeCollectionAddTarget | null) {
  return invoke<ThemeCollectionImport>("add_to_theme_collection", {
    id,
    path,
    target: target ?? null,
  });
}

export function getMediaSource(path: string) {
  return invoke<MediaSource>("get_media_source", { path }).then(withAssetUrl);
}

export function getMediaDataSource(path: string) {
  return invoke<MediaSource>("get_media_data_source", { path });
}

export function getThumbnailSource(path: string) {
  return invoke<MediaSource>("get_thumbnail_source", { path }).then(withAssetUrl);
}

export function getThumbnailDataSource(path: string) {
  return invoke<MediaSource>("get_thumbnail_data_source", { path });
}

export function getLivePreviewSource(path: string, quality: PreviewQuality) {
  return invoke<MediaSource>("get_live_preview_source", { path, quality }).then(withAssetUrl);
}

export function getLivePreviewDataSource(path: string, quality: PreviewQuality) {
  return invoke<MediaSource>("get_live_preview_data_source", { path, quality });
}

export function warmLivePreview(path: string) {
  return invoke<void>("warm_live_preview", { path });
}

export function saveLastFolder(path: string) {
  return invoke<void>("save_last_folder", { path });
}

export function saveLastMonitor(monitor: string) {
  return invoke<void>("save_last_monitor", { monitor });
}

export function saveLivePreview(livePreview: boolean) {
  return invoke<void>("save_live_preview", { livePreview });
}

export function savePreviewQuality(previewQuality: PreviewQuality) {
  return invoke<void>("save_preview_quality", { previewQuality });
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
