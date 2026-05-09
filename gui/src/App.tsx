import { useCallback, useEffect, useMemo, useState } from "react";
import EmptyState from "./components/EmptyState";
import FilmRoll from "./components/FilmRoll";
import PickerControls from "./components/PickerControls";
import PreviewStage from "./components/PreviewStage";
import {
  applyWallpaper,
  closePicker,
  getStatus,
  previewPlan,
  resolveStartupFolder,
  saveLastFolder,
  scanFolder,
  warmLivePreview,
} from "./lib/invoke";
import { shuffledIndex } from "./lib/media";
import type { JobowallsStatus, SetPlanPreview, WallpaperItem } from "./lib/types";

const DEFAULT_MONITOR = "all";

export default function App() {
  const [folder, setFolder] = useState<string | null>(null);
  const [items, setItems] = useState<WallpaperItem[]>([]);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [status, setStatus] = useState<JobowallsStatus | null>(null);
  const [plan, setPlan] = useState<SetPlanPreview | null>(null);
  const [monitor] = useState(DEFAULT_MONITOR);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [applyingPath, setApplyingPath] = useState<string | null>(null);

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
        await saveLastFolder(path);
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
      await loadStatus();
      try {
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

  useEffect(() => {
    if (items.length === 0) {
      return;
    }

    for (const offset of [-1, 0, 1]) {
      const item = items[wrapIndex(selectedIndex + offset, items.length)];
      if (item?.kind === "live") {
        void warmLivePreview(item.path);
      }
    }
  }, [items, selectedIndex]);

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
    const path = window.prompt("Folder path", folder ?? "");
    if (!path?.trim()) {
      return;
    }
    await loadFolder(path.trim());
  }, [folder, loadFolder]);

  const rescan = useCallback(async () => {
    if (folder) {
      await loadFolder(folder);
    }
  }, [folder, loadFolder]);

  const shuffle = useCallback(() => {
    setSelectedIndex((index) => shuffledIndex(items, index));
  }, [items]);

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
          onApply={() => void applySelected()}
          onShuffle={shuffle}
          onRescan={() => void rescan()}
          onFolderPrompt={() => void promptFolder()}
          onClose={() => void closePicker()}
        />
        <EmptyState message={emptyMessage} onFolderPrompt={() => void promptFolder()} />
      </div>
    );
  }

  return (
    <div
      className="app-shell"
      onWheel={(event) => {
        if (Math.abs(event.deltaX) > Math.abs(event.deltaY)) {
          selectRelative(event.deltaX > 0 ? 1 : -1);
        } else if (Math.abs(event.deltaY) > 20) {
          selectRelative(event.deltaY > 0 ? 1 : -1);
        }
      }}
    >
      <PickerControls
        folder={folder}
        planBackend={plan?.backend}
        monitor={plan?.monitor ?? monitor}
        count={items.length}
        onApply={() => void applySelected()}
        onShuffle={shuffle}
        onRescan={() => void rescan()}
        onFolderPrompt={() => void promptFolder()}
        onClose={() => void closePicker()}
      />

      <PreviewStage
        item={selected}
        activePath={activePath}
        applying={applyingPath === selected?.path}
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
    </div>
  );
}

function wrapIndex(index: number, length: number) {
  return ((index % length) + length) % length;
}
