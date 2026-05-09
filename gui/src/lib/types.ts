export type MediaKind = "static" | "live";

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
