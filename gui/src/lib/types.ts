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

export type GuiStartupOptions = {
  folder: string | null;
  monitor: string | null;
  livePreview: boolean | null;
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

export type GuiState = {
  version: number;
  lastFolder?: string | null;
  lastMonitor?: string | null;
  livePreview?: boolean | null;
  previewQuality?: PreviewQuality | null;
  previewMode?: boolean | null;
};

export type CollectionSummary = {
  id: string;
  name: string;
  count: number;
  source_folder?: string | null;
};

export type CollectionDetail = CollectionSummary & {
  items: WallpaperItem[];
};

export type ThemeCollectionSource = "user-theme" | "stock-theme" | "author-theme";
export type ThemeCollectionAddTarget = "user-backgrounds" | "theme-repo";

export type ThemeCollectionSummary = {
  id: string;
  name: string;
  theme_name: string;
  count: number;
  source: ThemeCollectionSource;
  installed_path: string;
  real_path: string;
  theme_backgrounds_path: string;
  user_backgrounds_path: string;
  can_write_user_backgrounds: boolean;
  can_write_theme_repo: boolean;
  add_requires_choice: boolean;
  default_add_target: ThemeCollectionAddTarget;
};

export type ThemeCollectionDetail = ThemeCollectionSummary & {
  items: WallpaperItem[];
};

export type ThemeCollectionImport = {
  collection: ThemeCollectionDetail;
  copied_path: string;
  target: ThemeCollectionAddTarget;
};
