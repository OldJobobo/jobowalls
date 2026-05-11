import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { open } from "@tauri-apps/plugin-dialog";
import EmptyState from "./components/EmptyState";
import FilmRoll from "./components/FilmRoll";
import PickerControls from "./components/PickerControls";
import PreviewStage from "./components/PreviewStage";
import {
  applyWallpaper,
  closePicker,
  getGuiConfig,
  getOmarchyThemeColors,
  getStatus,
  previewPlan,
  resolveStartupFolder,
  saveLastFolder,
  scanFolder,
} from "./lib/invoke";
import { shuffledIndex } from "./lib/media";
import type {
  GuiConfig,
  JobowallsStatus,
  OmarchyThemeColors,
  PreviewQuality,
  SetPlanPreview,
  WallpaperItem,
} from "./lib/types";

const DEFAULT_MONITOR = "all";
const PREVIEW_QUALITY_KEY = "jobowalls:previewQuality";
const DEFAULT_GUI_CONFIG: GuiConfig = {
  defaultMonitor: DEFAULT_MONITOR,
  previewQuality: "balanced",
  rememberLastFolder: true,
  useOmarchyTheme: true,
  windowWidth: 1040,
  windowHeight: 620,
  livePreview: true,
};

export default function App() {
  const [folder, setFolder] = useState<string | null>(null);
  const [items, setItems] = useState<WallpaperItem[]>([]);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [status, setStatus] = useState<JobowallsStatus | null>(null);
  const [plan, setPlan] = useState<SetPlanPreview | null>(null);
  const [monitor, setMonitor] = useState(DEFAULT_MONITOR);
  const [previewQuality, setPreviewQuality] = useState<PreviewQuality>(DEFAULT_GUI_CONFIG.previewQuality);
  const [livePreview, setLivePreview] = useState(DEFAULT_GUI_CONFIG.livePreview);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [applyingPath, setApplyingPath] = useState<string | null>(null);
  const lastWheelAt = useRef(0);
  const rememberLastFolder = useRef(DEFAULT_GUI_CONFIG.rememberLastFolder);

  const selected = items[selectedIndex] ?? null;
  const activePath = status?.wallpaper;

  const loadStatus = useCallback(async () => {
    try {
      setStatus(await getStatus());
    } catch {
      setStatus({ state_exists: false });
    }
  }, []);

  const loadFolder = useCallback(
    async (path: string) => {
      setLoading(true);
      setError(null);
      try {
        const scanned = await scanFolder(path);
        setFolder(path);
        setItems(scanned);
        setSelectedIndex((previous) => {
          if (scanned.length === 0) {
            return 0;
          }
          const activeIndex = activePath
            ? scanned.findIndex((item) => item.path === activePath)
            : -1;
          if (activeIndex >= 0) {
            return activeIndex;
          }
          return Math.min(previous, scanned.length - 1);
        });
        if (rememberLastFolder.current) {
          await saveLastFolder(path);
        }
      } catch (err) {
        setError(String(err));
        setItems([]);
      } finally {
        setLoading(false);
      }
    },
    [activePath],
  );

  useEffect(() => {
    let mounted = true;

    async function boot() {
      setLoading(true);
      try {
        const config = normalizeGuiConfig(await getGuiConfig());
        if (!mounted) {
          return;
        }
        rememberLastFolder.current = config.rememberLastFolder;
        setMonitor(config.defaultMonitor);
        setPreviewQuality(loadPreviewQuality(config.previewQuality));
        setLivePreview(config.livePreview);
        void getCurrentWindow().setSize(new LogicalSize(config.windowWidth, config.windowHeight));
        if (config.useOmarchyTheme) {
          try {
            const colors = await getOmarchyThemeColors();
            if (mounted && colors) {
              applyThemeColors(colors);
            }
          } catch {
            // Keep the built-in palette when Omarchy theme colors are unavailable.
          }
        }

        await loadStatus();
        const startup = await resolveStartupFolder();
        if (!mounted) {
          return;
        }
        if (startup.path) {
          await loadFolder(startup.path);
        } else {
          setLoading(false);
          setError("No wallpaper folder found.");
        }
      } catch (err) {
        if (mounted) {
          setLoading(false);
          setError(String(err));
        }
      }
    }

    void boot();
    return () => {
      mounted = false;
    };
  }, [loadFolder, loadStatus]);

  useEffect(() => {
    let cancelled = false;

    async function loadPlan() {
      if (!selected) {
        setPlan(null);
        return;
      }
      await new Promise((resolve) => window.setTimeout(resolve, 160));
      if (cancelled) {
        return;
      }
      try {
        const nextPlan = await previewPlan(selected.path, monitor);
        if (!cancelled) {
          setPlan(nextPlan);
        }
      } catch {
        if (!cancelled) {
          setPlan(null);
        }
      }
    }

    void loadPlan();
    return () => {
      cancelled = true;
    };
  }, [selected, monitor]);

  const selectRelative = useCallback(
    (step: number) => {
      if (items.length === 0) {
        return;
      }
      setSelectedIndex((index) => wrapIndex(index + step, items.length));
    },
    [items.length],
  );

  const applySelected = useCallback(
    async (index = selectedIndex) => {
      const item = items[index];
      if (!item) {
        return;
      }

      setApplyingPath(item.path);
      setError(null);
      try {
        await applyWallpaper(item.path, monitor);
        await loadStatus();
        await closePicker();
      } catch (err) {
        setError(String(err));
      } finally {
        setApplyingPath(null);
      }
    },
    [items, loadStatus, monitor, selectedIndex],
  );

  const promptFolder = useCallback(async () => {
    const selected = await open({
      directory: true,
      multiple: false,
      defaultPath: folder ?? undefined,
      title: "Choose wallpaper folder",
    });
    if (typeof selected !== "string" || !selected.trim()) {
      return;
    }

    await loadFolder(selected);
  }, [folder, loadFolder]);

  const rescan = useCallback(async () => {
    if (folder) {
      await loadFolder(folder);
    }
  }, [folder, loadFolder]);

  const shuffle = useCallback(() => {
    setSelectedIndex((index) => shuffledIndex(items, index));
  }, [items]);

  const changePreviewQuality = useCallback((quality: PreviewQuality) => {
    setPreviewQuality(quality);
    window.localStorage.setItem(PREVIEW_QUALITY_KEY, quality);
  }, []);

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      const key = event.key.toLowerCase();
      if (key === "arrowleft" || key === "h") {
        event.preventDefault();
        selectRelative(-1);
      } else if (key === "arrowright" || key === "l") {
        event.preventDefault();
        selectRelative(1);
      } else if (key === "enter") {
        event.preventDefault();
        void applySelected();
      } else if (key === "s") {
        event.preventDefault();
        shuffle();
      } else if (key === "r") {
        event.preventDefault();
        void rescan();
      } else if (key === "o") {
        event.preventDefault();
        void promptFolder();
      } else if (key === "escape") {
        event.preventDefault();
        void closePicker();
      }
    }

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [applySelected, promptFolder, rescan, selectRelative, shuffle]);

  const emptyMessage = useMemo(() => {
    if (loading) {
      return "Loading wallpapers.";
    }
    if (error) {
      return error;
    }
    if (folder && items.length === 0) {
      return "No supported wallpapers in this folder.";
    }
    return "Choose a wallpaper folder.";
  }, [error, folder, items.length, loading]);

  if (loading || items.length === 0) {
    return (
      <div className="app-shell">
        <PickerControls
          folder={folder}
          planBackend={plan?.backend}
          monitor={monitor}
          count={items.length}
          previewQuality={previewQuality}
          onApply={() => void applySelected()}
          onShuffle={shuffle}
          onRescan={() => void rescan()}
          onFolderPrompt={() => void promptFolder()}
          onPreviewQualityChange={changePreviewQuality}
          onClose={() => void closePicker()}
        />
        <EmptyState
          message={emptyMessage}
          loading={loading}
          showFolderButton={!loading}
          onFolderPrompt={() => void promptFolder()}
        />
      </div>
    );
  }

  return (
    <div
      className="app-shell"
      onWheel={(event) => {
        const now = window.performance.now();
        if (now - lastWheelAt.current < 120) {
          return;
        }

        if (Math.abs(event.deltaX) > Math.abs(event.deltaY)) {
          lastWheelAt.current = now;
          selectRelative(event.deltaX > 0 ? 1 : -1);
        } else if (Math.abs(event.deltaY) > 20) {
          lastWheelAt.current = now;
          selectRelative(event.deltaY > 0 ? 1 : -1);
        }
      }}
    >
      <PickerControls
        folder={folder}
        planBackend={plan?.backend}
        monitor={plan?.monitor ?? monitor}
        count={items.length}
        previewQuality={previewQuality}
        onApply={() => void applySelected()}
        onShuffle={shuffle}
        onRescan={() => void rescan()}
        onFolderPrompt={() => void promptFolder()}
        onPreviewQualityChange={changePreviewQuality}
        onClose={() => void closePicker()}
      />

      <PreviewStage
        item={selected}
        activePath={activePath}
        applying={applyingPath === selected?.path}
        previewQuality={previewQuality}
        livePreview={livePreview}
      />

      <FilmRoll
        items={items}
        selectedIndex={selectedIndex}
        activePath={activePath}
        applyingPath={applyingPath}
        onSelect={setSelectedIndex}
        onApply={(index) => void applySelected(index)}
      />

      {error && <div className="error-strip">{error}</div>}
      <button
        type="button"
        className="resize-grip"
        aria-label="Resize window"
        title="Resize"
        onPointerDown={(event) => {
          event.preventDefault();
          void getCurrentWindow().startResizeDragging("SouthEast");
        }}
      />
    </div>
  );
}

function wrapIndex(index: number, length: number) {
  return ((index % length) + length) % length;
}

function loadPreviewQuality(fallback: PreviewQuality): PreviewQuality {
  const value = window.localStorage.getItem(PREVIEW_QUALITY_KEY);
  return value === "fast" || value === "balanced" || value === "pretty" ? value : fallback;
}

function normalizeGuiConfig(config: GuiConfig): GuiConfig {
  return {
    ...DEFAULT_GUI_CONFIG,
    ...config,
    defaultMonitor: config.defaultMonitor.trim() || DEFAULT_MONITOR,
    windowWidth: config.windowWidth > 0 ? config.windowWidth : DEFAULT_GUI_CONFIG.windowWidth,
    windowHeight: config.windowHeight > 0 ? config.windowHeight : DEFAULT_GUI_CONFIG.windowHeight,
  };
}

function applyThemeColors(colors: OmarchyThemeColors) {
  const root = document.documentElement;
  root.style.setProperty("--jw-background", colors.background);
  root.style.setProperty("--jw-foreground", colors.foreground);
  root.style.setProperty("--jw-accent", colors.accent);
  root.style.setProperty("--jw-selection-background", colors.selectionBackground);
  root.style.setProperty("--jw-selection-foreground", colors.selectionForeground);
  root.style.setProperty("--jw-muted", colors.muted);
  root.style.setProperty("--jw-surface", colors.surface);
  root.style.setProperty("--jw-surface-raised", colors.surfaceRaised);
  root.style.setProperty("--jw-warning", colors.warning);
  root.style.setProperty("--jw-error", colors.error);
}
