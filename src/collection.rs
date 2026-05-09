use crate::{media::has_supported_extension, state::CollectionState};
use anyhow::{Context, Result, bail};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn scan_collection(path: &Path) -> Result<Vec<PathBuf>> {
    if !path.is_dir() {
        bail!("collection path is not a directory: {}", path.display());
    }

    let mut wallpapers = Vec::new();
    for entry in fs::read_dir(path)
        .with_context(|| format!("failed to read collection {}", path.display()))?
    {
        let entry = entry.with_context(|| format!("failed to read entry in {}", path.display()))?;
        let path = entry.path();
        if path.is_file() && has_supported_extension(&path) {
            wallpapers.push(fs::canonicalize(&path).unwrap_or(path));
        }
    }

    wallpapers.sort();
    wallpapers.dedup();

    if wallpapers.is_empty() {
        bail!("collection has no supported wallpapers: {}", path.display());
    }

    Ok(wallpapers)
}

pub fn select_next(collection: &[PathBuf], current: Option<&Path>) -> PathBuf {
    select_relative(collection, current, 1)
}

pub fn select_previous(collection: &[PathBuf], current: Option<&Path>) -> PathBuf {
    select_relative(collection, current, -1)
}

pub fn select_shuffle(collection: &[PathBuf], current: Option<&Path>, seed: u64) -> PathBuf {
    if collection.len() == 1 {
        return collection[0].clone();
    }

    let current_index = current.and_then(|current| index_of(collection, current));
    let mut index = (seed as usize) % collection.len();
    if Some(index) == current_index {
        index = (index + 1) % collection.len();
    }

    collection[index].clone()
}

pub fn index_of_path(collection: &[PathBuf], current: &Path) -> Option<usize> {
    index_of(collection, current)
}

pub fn select_next_persistent(
    collection: &[PathBuf],
    collection_state: Option<&CollectionState>,
    fallback_current: Option<&Path>,
) -> (usize, PathBuf) {
    select_relative_persistent(collection, collection_state, fallback_current, 1)
}

pub fn select_previous_persistent(
    collection: &[PathBuf],
    collection_state: Option<&CollectionState>,
    fallback_current: Option<&Path>,
) -> (usize, PathBuf) {
    select_relative_persistent(collection, collection_state, fallback_current, -1)
}

pub fn select_shuffle_persistent(
    collection: &[PathBuf],
    collection_state: Option<&CollectionState>,
    fallback_current: Option<&Path>,
    seed: u64,
) -> (usize, PathBuf) {
    if collection.len() == 1 {
        return (0, collection[0].clone());
    }

    let history = collection_state
        .map(|state| state.shuffle_history.as_slice())
        .unwrap_or(&[]);
    let mut candidates = collection
        .iter()
        .enumerate()
        .filter(|(_, path)| !history.iter().any(|seen| Path::new(seen) == path.as_path()))
        .collect::<Vec<_>>();

    if candidates.is_empty() {
        candidates = collection.iter().enumerate().collect();
    }

    let current_index = current_index(collection, collection_state, fallback_current);
    let mut selected = (seed as usize) % candidates.len();
    if candidates.len() > 1 && Some(candidates[selected].0) == current_index {
        selected = (selected + 1) % candidates.len();
    }

    let (index, path) = candidates[selected];
    (index, path.clone())
}

fn select_relative(collection: &[PathBuf], current: Option<&Path>, step: isize) -> PathBuf {
    if collection.is_empty() {
        return PathBuf::new();
    }

    let current_index = current
        .and_then(|current| index_of(collection, current))
        .unwrap_or(if step >= 0 { collection.len() - 1 } else { 0 });

    let len = collection.len() as isize;
    let index = (current_index as isize + step).rem_euclid(len) as usize;
    collection[index].clone()
}

fn select_relative_persistent(
    collection: &[PathBuf],
    collection_state: Option<&CollectionState>,
    fallback_current: Option<&Path>,
    step: isize,
) -> (usize, PathBuf) {
    let current_index = current_index(collection, collection_state, fallback_current)
        .unwrap_or(if step >= 0 { collection.len() - 1 } else { 0 });

    let len = collection.len() as isize;
    let index = (current_index as isize + step).rem_euclid(len) as usize;
    (index, collection[index].clone())
}

fn current_index(
    collection: &[PathBuf],
    collection_state: Option<&CollectionState>,
    fallback_current: Option<&Path>,
) -> Option<usize> {
    collection_state
        .and_then(|state| state.last_index)
        .filter(|index| *index < collection.len())
        .or_else(|| {
            collection_state
                .and_then(|state| state.last_wallpaper.as_deref())
                .and_then(|wallpaper| index_of(collection, Path::new(wallpaper)))
        })
        .or_else(|| fallback_current.and_then(|current| index_of(collection, current)))
}

fn index_of(collection: &[PathBuf], current: &Path) -> Option<usize> {
    let current = fs::canonicalize(current).unwrap_or_else(|_| current.to_path_buf());
    collection.iter().position(|path| path == &current)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn scans_supported_files_sorted() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("b.mp4"), "").unwrap();
        fs::write(dir.path().join("a.jpg"), "").unwrap();
        fs::write(dir.path().join("notes.txt"), "").unwrap();

        let files = scan_collection(dir.path()).unwrap();

        assert_eq!(files.len(), 2);
        assert!(files[0].ends_with("a.jpg"));
        assert!(files[1].ends_with("b.mp4"));
    }

    #[test]
    fn selects_next_and_previous_with_wraparound() {
        let files = vec![
            "/tmp/a.jpg".into(),
            "/tmp/b.jpg".into(),
            "/tmp/c.jpg".into(),
        ];

        assert_eq!(
            select_next(&files, Some(Path::new("/tmp/b.jpg"))),
            PathBuf::from("/tmp/c.jpg")
        );
        assert_eq!(
            select_previous(&files, Some(Path::new("/tmp/a.jpg"))),
            PathBuf::from("/tmp/c.jpg")
        );
    }

    #[test]
    fn shuffle_avoids_current_when_possible() {
        let files = vec!["/tmp/a.jpg".into(), "/tmp/b.jpg".into()];

        assert_eq!(
            select_shuffle(&files, Some(Path::new("/tmp/a.jpg")), 0),
            PathBuf::from("/tmp/b.jpg")
        );
    }

    #[test]
    fn persistent_next_uses_saved_index() {
        let files = vec![
            "/tmp/a.jpg".into(),
            "/tmp/b.jpg".into(),
            "/tmp/c.jpg".into(),
        ];
        let state = CollectionState {
            last_index: Some(1),
            last_wallpaper: Some("/elsewhere/not-current.jpg".to_string()),
            shuffle_history: Vec::new(),
        };

        let (index, path) = select_next_persistent(&files, Some(&state), None);

        assert_eq!(index, 2);
        assert_eq!(path, PathBuf::from("/tmp/c.jpg"));
    }

    #[test]
    fn persistent_shuffle_skips_seen_until_exhausted() {
        let files = vec![
            "/tmp/a.jpg".into(),
            "/tmp/b.jpg".into(),
            "/tmp/c.jpg".into(),
        ];
        let state = CollectionState {
            last_index: Some(0),
            last_wallpaper: Some("/tmp/a.jpg".to_string()),
            shuffle_history: vec!["/tmp/a.jpg".to_string(), "/tmp/b.jpg".to_string()],
        };

        let (index, path) = select_shuffle_persistent(&files, Some(&state), None, 0);

        assert_eq!(index, 2);
        assert_eq!(path, PathBuf::from("/tmp/c.jpg"));
    }
}
