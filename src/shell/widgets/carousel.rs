use crate::shell::{
    model::WallpaperItem,
    widgets::thumbnail::{self, ThumbnailRole},
};
use gtk::prelude::*;
use std::path::Path;

pub fn build(
    items: &[WallpaperItem],
    selected: usize,
    active_wallpaper: Option<&Path>,
    animate_live: bool,
    animation_offset: i32,
    animation_opacity: f64,
) -> gtk::Box {
    let root = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    root.add_css_class("carousel");
    root.set_halign(gtk::Align::Center);
    root.set_valign(gtk::Align::Center);
    root.set_opacity(animation_opacity);
    if animation_offset > 0 {
        root.set_margin_start(animation_offset);
    } else if animation_offset < 0 {
        root.set_margin_end(animation_offset.abs());
    }

    if items.is_empty() {
        return root;
    }

    let len = items.len();
    let selected = selected % len;
    let far_previous = offset_index(len, selected, -2);
    let previous = offset_index(len, selected, -1);
    let next = offset_index(len, selected, 1);
    let far_next = offset_index(len, selected, 2);

    root.append(&thumbnail::build(
        far_previous.map(|index| &items[index]),
        ThumbnailRole::FarPrevious,
        is_active(items, far_previous, active_wallpaper),
        animate_live,
    ));
    root.append(&thumbnail::build(
        previous.map(|index| &items[index]),
        ThumbnailRole::Previous,
        is_active(items, previous, active_wallpaper),
        animate_live,
    ));
    root.append(&thumbnail::build(
        Some(&items[selected]),
        ThumbnailRole::Selected,
        active_wallpaper
            .map(|active| items[selected].path == active)
            .unwrap_or(false),
        animate_live,
    ));
    root.append(&thumbnail::build(
        next.map(|index| &items[index]),
        ThumbnailRole::Next,
        is_active(items, next, active_wallpaper),
        animate_live,
    ));
    root.append(&thumbnail::build(
        far_next.map(|index| &items[index]),
        ThumbnailRole::FarNext,
        is_active(items, far_next, active_wallpaper),
        animate_live,
    ));

    root
}

fn offset_index(len: usize, selected: usize, offset: isize) -> Option<usize> {
    if len < 2 {
        return None;
    }

    let distance = offset.unsigned_abs();
    if distance >= len {
        return None;
    }

    if len < 5 && distance == 2 {
        return None;
    }

    let index = if offset < 0 {
        (selected + len - distance) % len
    } else {
        (selected + distance) % len
    };
    Some(index)
}

fn is_active(
    items: &[WallpaperItem],
    index: Option<usize>,
    active_wallpaper: Option<&Path>,
) -> bool {
    index
        .and_then(|index| active_wallpaper.map(|active| items[index].path == active))
        .unwrap_or(false)
}
