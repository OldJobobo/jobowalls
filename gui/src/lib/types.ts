export type MediaKind = "static" | "live";
export type PreviewQuality = "fast" | "balanced" | "pretty";

export type WallpaperItem = {
  path: string;
  name: string;
  kind: MediaKind;
};

export type StartupFolder = {
  path: string | null;
  source: string;
};

export type SetPlanPreview = {
  wallpaper: string;
  media_kind: MediaKind;
  backend: string;
  monitor: string;
};

export type JobowallsStatus = {
  state_exists: boolean;
  active_backend?: string;
  mode?: MediaKind;
  wallpaper?: string;
  monitors?: Record<
    string,
    {
      backend: string;
      wallpaper: string;
      pid: number | null;
    }
  >;
  updated_at?: string;
};

export type MediaSource = {
  path: string;
  src: string | null;
  reason: string | null;
};

export type OmarchyThemeColors = {
  background: string;
  foreground: string;
  accent: string;
  selectionBackground: string;
  selectionForeground: string;
  muted: string;
  surface: string;
  surfaceRaised: string;
  warning: string;
  error: string;
};

export type GuiConfig = {
  defaultMonitor: string;
  previewQuality: PreviewQuality;
  rememberLastFolder: boolean;
  useOmarchyTheme: boolean;
  windowWidth: number;
  windowHeight: number;
  livePreview: boolean;
};
