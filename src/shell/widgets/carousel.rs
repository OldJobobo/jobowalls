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
) -> gtk::Box {
    let root = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    root.add_css_class("carousel");
    root.set_halign(gtk::Align::Center);
    root.set_valign(gtk::Align::Center);

    if items.is_empty() {
        return root;
    }

    let len = items.len();
    let selected = selected % len;
    let previous = if len > 1 {
        Some((selected + len - 1) % len)
    } else {
        None
    };
    let next = if len > 2 {
        Some((selected + 1) % len)
    } else if len == 2 {
        Some((selected + 1) % len)
    } else {
        None
    };

    root.append(&thumbnail::build(
        previous.map(|index| &items[index]),
        ThumbnailRole::Previous,
        previous
            .and_then(|index| active_wallpaper.map(|active| items[index].path == active))
            .unwrap_or(false),
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
        next.and_then(|index| active_wallpaper.map(|active| items[index].path == active))
            .unwrap_or(false),
        animate_live,
    ));

    root
}
