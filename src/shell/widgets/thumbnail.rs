use crate::shell::{model::WallpaperItem, preview};
use gtk::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThumbnailRole {
    Previous,
    Selected,
    Next,
}

pub fn build(
    item: Option<&WallpaperItem>,
    role: ThumbnailRole,
    active: bool,
    animate_live: bool,
) -> gtk::Box {
    let root = gtk::Box::new(gtk::Orientation::Vertical, 4);
    root.add_css_class("thumb");
    match role {
        ThumbnailRole::Selected => root.add_css_class("selected"),
        ThumbnailRole::Previous | ThumbnailRole::Next => root.add_css_class("neighbor"),
    }
    if active {
        root.add_css_class("active");
    }
    root.set_width_request(if role == ThumbnailRole::Selected {
        240
    } else {
        132
    });
    root.set_height_request(if role == ThumbnailRole::Selected {
        135
    } else {
        74
    });

    if let Some(item) = item {
        let selected = role == ThumbnailRole::Selected;
        let display_path = preview::display_path(item, selected, animate_live);
        let image = gtk::Picture::for_filename(display_path);
        image.set_can_shrink(true);
        root.append(&image);

        if item.is_live() {
            let label = gtk::Label::new(Some("LIVE"));
            label.add_css_class("live-label");
            root.append(&label);
        }
    }

    root
}
