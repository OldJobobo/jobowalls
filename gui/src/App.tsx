import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { open } from "@tauri-apps/plugin-dialog";
import EmptyState from "./components/EmptyState";
import FilmRoll from "./components/FilmRoll";
import { clearMediaPreviewCache } from "./components/MediaPreview";
import PickerControls from "./components/PickerControls";
import PreviewStage from "./components/PreviewStage";
import {
  addToCollection,
  addToThemeCollection,
  adoptOmarchyBackground,
  applyWallpaper,
  closePicker,
  createCollection,
  getGuiConfig,
  getGuiState,
  getCollection,
  getMonitorNames,
  getOmarchyThemeColors,
  getStartupOptions,
  getStatus,
  getThemeCollection,
  listCollections,
  listThemeCollections,
  previewPlan,
  resolveStartupFolder,
  removeFromCollection,
  saveLastFolder,
  saveLastMonitor,
  saveLivePreview,
  savePreviewQuality,
  scanFolder,
} from "./lib/invoke";
import { shuffledIndex } from "./lib/media";
import type {
  CollectionSummary,
  GuiConfig,
  JobowallsStatus,
  OmarchyThemeColors,
  PreviewQuality,
  SetPlanPreview,
  ThemeCollectionAddTarget,
  ThemeCollectionSummary,
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

type CollectionDialogState =
  | { mode: "create"; name: string; targetId: string }
  | { mode: "create-and-add"; name: string; targetId: string }
  | { mode: "add"; name: string; targetId: string; themeTarget: ThemeCollectionAddTarget };

export default function App() {
  const [folder, setFolder] = useState<string | null>(null);
  const [items, setItems] = useState<WallpaperItem[]>([]);
  const [collections, setCollections] = useState<CollectionSummary[]>([]);
  const [themeCollections, setThemeCollections] = useState<ThemeCollectionSummary[]>([]);
  const [activeCollectionId, setActiveCollectionId] = useState<string | null>(null);
  const [sourceLabel, setSourceLabel] = useState<string | null>(null);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [status, setStatus] = useState<JobowallsStatus | null>(null);
  const [plan, setPlan] = useState<SetPlanPreview | null>(null);
  const [monitor, setMonitor] = useState(DEFAULT_MONITOR);
  const [monitorNames, setMonitorNames] = useState<string[]>([]);
  const [previewQuality, setPreviewQuality] = useState<PreviewQuality>(DEFAULT_GUI_CONFIG.previewQuality);
  const [livePreview, setLivePreview] = useState(DEFAULT_GUI_CONFIG.livePreview);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [applyingPath, setApplyingPath] = useState<string | null>(null);
  const [collectionDialog, setCollectionDialog] = useState<CollectionDialogState | null>(null);
  const [collectionDialogBusy, setCollectionDialogBusy] = useState(false);
  const lastWheelAt = useRef(0);
  const rememberLastFolder = useRef(DEFAULT_GUI_CONFIG.rememberLastFolder);
  const activePathRef = useRef<string | undefined>(undefined);
  const folderRef = useRef<string | null>(null);

  const selected = items[selectedIndex] ?? null;
  const activePath = status?.wallpaper;
  const themeById = useMemo(
    () => new Map(themeCollections.map((collection) => [collection.id, collection])),
    [themeCollections],
  );
  const sourceOptions = useMemo(
    () => [
      ...collections.map((collection) => ({ id: collection.id, label: collection.name })),
      ...themeCollections.map((collection) => ({ id: collection.id, label: `Theme: ${collection.name}` })),
    ],
    [collections, themeCollections],
  );
  const activeSourceIsTheme = activeCollectionId?.startsWith("theme:") ?? false;

  const loadCollections = useCallback(async () => {
    try {
      const nextCollections = await listCollections();
      setCollections(nextCollections);
      return nextCollections;
    } catch {
      setCollections([]);
      return [];
    }
  }, []);

  const loadThemeCollections = useCallback(async () => {
    try {
      const nextCollections = await listThemeCollections();
      setThemeCollections(nextCollections);
      return nextCollections;
    } catch {
      setThemeCollections([]);
      return [];
    }
  }, []);

  const loadStatus = useCallback(async () => {
    try {
      const nextStatus = await getStatus();
      activePathRef.current = nextStatus.wallpaper;
      setStatus(nextStatus);
      return nextStatus;
    } catch {
      const fallback: JobowallsStatus = { state_exists: false };
      activePathRef.current = undefined;
      setStatus(fallback);
      return fallback;
    }
  }, []);

  const loadFolder = useCallback(
    async (path: string, activePathOverride: string | null | undefined = activePathRef.current) => {
      setLoading(true);
      setError(null);
      try {
        if (path !== folderRef.current) {
          clearMediaPreviewCache();
        }
        const scanned = await scanFolder(path);
        folderRef.current = path;
        setFolder(path);
        setActiveCollectionId(null);
        setSourceLabel(null);
        setItems(scanned);
        setSelectedIndex((previous) => {
          if (scanned.length === 0) {
            return 0;
          }
          const activeIndex = activePathOverride
            ? scanned.findIndex((item) => item.path === activePathOverride)
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
    [],
  );

  const loadCollection = useCallback(
    async (id: string, activePathOverride: string | null | undefined = activePathRef.current) => {
      setLoading(true);
      setError(null);
      try {
        clearMediaPreviewCache();
        const collection = await getCollection(id);
        setActiveCollectionId(id);
        setSourceLabel(collection.name);
        setItems(collection.items);
        setSelectedIndex((previous) => {
          if (collection.items.length === 0) {
            return 0;
          }
          const activeIndex = activePathOverride
            ? collection.items.findIndex((item) => item.path === activePathOverride)
            : -1;
          if (activeIndex >= 0) {
            return activeIndex;
          }
          return Math.min(previous, collection.items.length - 1);
        });
      } catch (err) {
        setError(String(err));
        setItems([]);
      } finally {
        setLoading(false);
      }
    },
    [],
  );

  const loadThemeCollection = useCallback(
    async (id: string, activePathOverride: string | null | undefined = activePathRef.current) => {
      setLoading(true);
      setError(null);
      try {
        clearMediaPreviewCache();
        const collection = await getThemeCollection(id);
        setActiveCollectionId(id);
        setSourceLabel(`Theme: ${collection.name}`);
        setItems(collection.items);
        setSelectedIndex((previous) => {
          if (collection.items.length === 0) {
            return 0;
          }
          const activeIndex = activePathOverride
            ? collection.items.findIndex((item) => item.path === activePathOverride)
            : -1;
          if (activeIndex >= 0) {
            return activeIndex;
          }
          return Math.min(previous, collection.items.length - 1);
        });
      } catch (err) {
        setError(String(err));
        setItems([]);
      } finally {
        setLoading(false);
      }
    },
    [],
  );

  useEffect(() => {
    let mounted = true;

    async function boot() {
      setLoading(true);
      try {
        const [rawConfig, startupOptions, guiState, monitors, loadedCollections, loadedThemeCollections] =
          await Promise.all([
          getGuiConfig(),
          getStartupOptions(),
          getGuiState(),
          getMonitorNames().catch(() => []),
          listCollections().catch(() => []),
          listThemeCollections().catch(() => []),
        ]);
        const config = normalizeGuiConfig(rawConfig);
        if (!mounted) {
          return;
        }
        rememberLastFolder.current = config.rememberLastFolder;
        setCollections(loadedCollections);
        setThemeCollections(loadedThemeCollections);
        setMonitorNames(monitors);
        setMonitor(normalizeMonitor(startupOptions.monitor ?? guiState.lastMonitor ?? config.defaultMonitor));
        setPreviewQuality(guiState.previewQuality ?? loadPreviewQuality(config.previewQuality));
        setLivePreview(startupOptions.livePreview ?? guiState.livePreview ?? config.livePreview);
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

        await adoptOmarchyBackground().catch(() => null);
        const startupStatus = await loadStatus();
        const startup = await resolveStartupFolder(startupOptions.folder);
        if (!mounted) {
          return;
        }
        if (startup.path) {
          await loadFolder(startup.path, startupStatus.wallpaper ?? null);
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
    if (activeCollectionId) {
      if (activeSourceIsTheme) {
        await loadThemeCollection(activeCollectionId);
      } else {
        await loadCollection(activeCollectionId);
      }
    } else if (folder) {
      await loadFolder(folder);
    }
  }, [activeCollectionId, activeSourceIsTheme, folder, loadCollection, loadFolder, loadThemeCollection]);

  const shuffle = useCallback(() => {
    setSelectedIndex((index) => shuffledIndex(items, index));
  }, [items]);

  const changePreviewQuality = useCallback((quality: PreviewQuality) => {
    setPreviewQuality(quality);
    window.localStorage.setItem(PREVIEW_QUALITY_KEY, quality);
    void savePreviewQuality(quality);
  }, []);

  const changeMonitor = useCallback((nextMonitor: string) => {
    const normalized = normalizeMonitor(nextMonitor);
    setMonitor(normalized);
    void saveLastMonitor(normalized);
  }, []);

  const changeLivePreview = useCallback((enabled: boolean) => {
    setLivePreview(enabled);
    void saveLivePreview(enabled);
  }, []);

  const changeCollection = useCallback(
    (id: string | null) => {
      if (id) {
        if (id.startsWith("theme:")) {
          void loadThemeCollection(id);
        } else {
          void loadCollection(id);
        }
      } else if (folder) {
        void loadFolder(folder);
      } else {
        setActiveCollectionId(null);
        setSourceLabel(null);
      }
    },
    [folder, loadCollection, loadFolder, loadThemeCollection],
  );

  const openCreateCollectionDialog = useCallback(() => {
    setCollectionDialog({ mode: "create", name: "", targetId: "" });
  }, []);

  const addSelectedToCollection = useCallback(
    async (targetId: string) => {
      if (!selected) {
        return;
      }

      setError(null);
      const detail = await addToCollection(targetId, [selected.path]);
      await loadCollections();
      if (activeCollectionId === targetId) {
        setItems(detail.items);
      }
    },
    [activeCollectionId, loadCollections, selected],
  );

  const addSelectedToThemeCollection = useCallback(
    async (targetId: string, target?: ThemeCollectionAddTarget | null) => {
      if (!selected) {
        return;
      }

      setError(null);
      const result = await addToThemeCollection(targetId, selected.path, target);
      await loadThemeCollections();
      if (activeCollectionId === targetId) {
        setItems(result.collection.items);
      }
    },
    [activeCollectionId, loadThemeCollections, selected],
  );

  const addSelectedToSource = useCallback(
    async (targetId: string, target?: ThemeCollectionAddTarget | null) => {
      if (targetId.startsWith("theme:")) {
        await addSelectedToThemeCollection(targetId, target);
      } else {
        await addSelectedToCollection(targetId);
      }
    },
    [addSelectedToCollection, addSelectedToThemeCollection],
  );

  const openAddToCollectionDialog = useCallback(async () => {
    if (!selected) {
      return;
    }

    if (activeCollectionId) {
      const activeTheme = themeById.get(activeCollectionId);
      if (activeTheme?.add_requires_choice) {
        setCollectionDialog({
          mode: "add",
          name: "",
          targetId: activeCollectionId,
          themeTarget: activeTheme.default_add_target,
        });
        return;
      }
      try {
        await addSelectedToSource(activeCollectionId);
      } catch (err) {
        setError(String(err));
      }
      return;
    }

    if (sourceOptions.length === 1) {
      const onlySource = sourceOptions[0];
      const onlyTheme = themeById.get(onlySource.id);
      if (onlyTheme?.add_requires_choice) {
        setCollectionDialog({
          mode: "add",
          name: "",
          targetId: onlySource.id,
          themeTarget: onlyTheme.default_add_target,
        });
        return;
      }
      try {
        await addSelectedToSource(onlySource.id);
      } catch (err) {
        setError(String(err));
      }
      return;
    }

    if (sourceOptions.length > 1) {
      const firstTheme = themeById.get(sourceOptions[0].id);
      setCollectionDialog({
        mode: "add",
        name: "",
        targetId: sourceOptions[0].id,
        themeTarget: firstTheme?.default_add_target ?? "user-backgrounds",
      });
      return;
    }

    setCollectionDialog({ mode: "create-and-add", name: "", targetId: "" });
  }, [activeCollectionId, addSelectedToSource, selected, sourceOptions, themeById]);

  const submitCollectionDialog = useCallback(async () => {
    if (!collectionDialog) {
      return;
    }

    setCollectionDialogBusy(true);
    setError(null);
    try {
      if (collectionDialog.mode === "create" || collectionDialog.mode === "create-and-add") {
        const name = collectionDialog.name.trim();
        if (!name) {
          setError("Collection name is required.");
          return;
        }
        const collection = await createCollection(name);
        await loadCollections();
        setCollectionDialog(null);
        if (collectionDialog.mode === "create-and-add" && selected) {
          await addSelectedToSource(collection.id);
        }
      } else {
        if (!collectionDialog.targetId) {
          setError("Choose a collection.");
          return;
        }
        await addSelectedToSource(collectionDialog.targetId, collectionDialog.themeTarget);
        setCollectionDialog(null);
      }
    } catch (err) {
      setError(String(err));
    } finally {
      setCollectionDialogBusy(false);
    }
  }, [addSelectedToSource, collectionDialog, loadCollections, selected]);

  const removeSelectedFromCollection = useCallback(async () => {
    if (!activeCollectionId || !selected) {
      return;
    }

    setError(null);
    try {
      const detail = await removeFromCollection(activeCollectionId, selected.path);
      await loadCollections();
      setItems(detail.items);
      setSelectedIndex((index) => Math.min(index, Math.max(0, detail.items.length - 1)));
    } catch (err) {
      setError(String(err));
    }
  }, [activeCollectionId, loadCollections, selected]);

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      const key = event.key.toLowerCase();
      if (collectionDialog) {
        if (key === "escape") {
          event.preventDefault();
          setCollectionDialog(null);
        }
        return;
      }
      if (isEditableEventTarget(event.target)) {
        return;
      }
      if (key === "arrowleft" || key === "h") {
        event.preventDefault();
        selectRelative(-1);
      } else if (key === "arrowright" || key === "l") {
        event.preventDefault();
        selectRelative(1);
      } else if (key === "enter") {
        event.preventDefault();
        void applySelected();
      } else if (key === " " || key === "spacebar" || event.code === "Space") {
        event.preventDefault();
        changeLivePreview(!livePreview);
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
  }, [
    applySelected,
    changeLivePreview,
    collectionDialog,
    livePreview,
    promptFolder,
    rescan,
    selectRelative,
    shuffle,
  ]);

  const dialogTheme =
    collectionDialog?.mode === "add" ? themeById.get(collectionDialog.targetId) : undefined;
  const collectionDialogElement = collectionDialog ? (
    <div className="modal-backdrop" role="presentation" onMouseDown={() => setCollectionDialog(null)}>
      <form
        className="collection-dialog"
        onMouseDown={(event) => event.stopPropagation()}
        onSubmit={(event) => {
          event.preventDefault();
          void submitCollectionDialog();
        }}
      >
        <div className="collection-dialog-header">
          <h2>{collectionDialogTitle(collectionDialog.mode)}</h2>
          <button type="button" onClick={() => setCollectionDialog(null)} aria-label="Close collection dialog">
            X
          </button>
        </div>

        {collectionDialog.mode === "add" ? (
          <label className="collection-dialog-field">
            <span>Collection</span>
            <select
              autoFocus
              value={collectionDialog.targetId}
              onChange={(event) =>
                setCollectionDialog((current) =>
                  current && current.mode === "add"
                    ? {
                        ...current,
                        targetId: event.target.value,
                        themeTarget: themeById.get(event.target.value)?.default_add_target ?? "user-backgrounds",
                      }
                    : current,
                )
              }
            >
                  {sourceOptions.map((source) => (
                    <option key={source.id} value={source.id}>
                      {source.label}
                    </option>
                  ))}
            </select>
            {dialogTheme?.add_requires_choice && (
              <>
                <span>Target</span>
                <select
                  value={collectionDialog.themeTarget}
                  onChange={(event) =>
                    setCollectionDialog((current) =>
                      current && current.mode === "add"
                        ? { ...current, themeTarget: event.target.value as ThemeCollectionAddTarget }
                        : current,
                    )
                  }
                >
                  <option value="user-backgrounds">User backgrounds</option>
                  <option value="theme-repo">Theme repo</option>
                </select>
              </>
            )}
          </label>
        ) : (
          <label className="collection-dialog-field">
            <span>Name</span>
            <input
              autoFocus
              value={collectionDialog.name}
              placeholder="Favorites"
              onChange={(event) =>
                setCollectionDialog((current) =>
                  current && current.mode !== "add" ? { ...current, name: event.target.value } : current,
                )
              }
            />
          </label>
        )}

        <div className="collection-dialog-actions">
          <button type="button" onClick={() => setCollectionDialog(null)}>
            Cancel
          </button>
          <button type="submit" disabled={collectionDialogBusy}>
            {collectionDialog.mode === "add" ? "Add" : "Create"}
          </button>
        </div>
      </form>
    </div>
  ) : null;

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
          sourceLabel={sourceLabel}
          planBackend={plan?.backend}
          monitor={monitor}
          monitorNames={monitorNames}
          count={items.length}
          collections={collections}
          themeCollections={themeCollections}
          activeCollectionId={activeCollectionId}
          selectedPath={selected?.path}
          canRemoveSelected={Boolean(selected?.path && activeCollectionId && !activeSourceIsTheme)}
          previewQuality={previewQuality}
          livePreview={livePreview}
          onApply={() => void applySelected()}
          onShuffle={shuffle}
          onRescan={() => void rescan()}
          onFolderPrompt={() => void promptFolder()}
          onCollectionChange={changeCollection}
          onCreateCollection={openCreateCollectionDialog}
          onAddToCollection={() => void openAddToCollectionDialog()}
          onRemoveFromCollection={() => void removeSelectedFromCollection()}
          onMonitorChange={changeMonitor}
          onPreviewQualityChange={changePreviewQuality}
          onLivePreviewChange={changeLivePreview}
          onClose={() => void closePicker()}
        />
        <EmptyState
          message={emptyMessage}
          loading={loading}
          showFolderButton={!loading}
          onFolderPrompt={() => void promptFolder()}
        />
        {collectionDialogElement}
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
        sourceLabel={sourceLabel}
        planBackend={plan?.backend}
        monitor={monitor}
        monitorNames={monitorNames}
        count={items.length}
        collections={collections}
        themeCollections={themeCollections}
        activeCollectionId={activeCollectionId}
        selectedPath={selected?.path}
        canRemoveSelected={Boolean(selected?.path && activeCollectionId && !activeSourceIsTheme)}
        previewQuality={previewQuality}
        livePreview={livePreview}
        onApply={() => void applySelected()}
        onShuffle={shuffle}
        onRescan={() => void rescan()}
        onFolderPrompt={() => void promptFolder()}
        onCollectionChange={changeCollection}
        onCreateCollection={openCreateCollectionDialog}
        onAddToCollection={() => void openAddToCollectionDialog()}
        onRemoveFromCollection={() => void removeSelectedFromCollection()}
        onMonitorChange={changeMonitor}
        onPreviewQualityChange={changePreviewQuality}
        onLivePreviewChange={changeLivePreview}
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
      {collectionDialogElement}
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

function normalizeMonitor(monitor: string) {
  return monitor.trim() || DEFAULT_MONITOR;
}

function collectionDialogTitle(mode: CollectionDialogState["mode"]) {
  if (mode === "add") {
    return "Add To Collection";
  }
  return "Create Collection";
}

function isEditableEventTarget(target: EventTarget | null) {
  if (!(target instanceof HTMLElement)) {
    return false;
  }

  const tagName = target.tagName.toLowerCase();
  return tagName === "input" || tagName === "textarea" || tagName === "select" || target.isContentEditable;
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
