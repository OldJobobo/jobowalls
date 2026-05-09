use crate::shell::{model::WallpaperItem, preview};
use gtk::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThumbnailRole {
    FarPrevious,
    Previous,
    Selected,
    Next,
    FarNext,
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
        ThumbnailRole::FarPrevious | ThumbnailRole::FarNext => root.add_css_class("far-neighbor"),
    }
    if active {
        root.add_css_class("active");
    }
    let (width, height) = match role {
        ThumbnailRole::Selected => (250, 141),
        ThumbnailRole::Previous | ThumbnailRole::Next => (150, 84),
        ThumbnailRole::FarPrevious | ThumbnailRole::FarNext => (108, 61),
    };
    root.set_width_request(width);
    root.set_height_request(height);

    if let Some(item) = item {
        let selected = role == ThumbnailRole::Selected;
        let display_path = preview::display_path(item, selected, animate_live);
        let image = gtk::Picture::for_filename(display_path);
        image.add_css_class("thumb-image");
        if selected {
            image.add_css_class("selected-image");
        }
        image.set_can_shrink(true);
        image.set_width_request(width);
        image.set_height_request(height);
        root.append(&image);

        if item.is_live() {
            let label = gtk::Label::new(Some("LIVE"));
            label.add_css_class("live-label");
            root.append(&label);
        }
    }

    root
}
