use crate::shell::{model::WallpaperItem, preview};
use gtk::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThumbnailRole {
    OuterPrevious,
    FarPrevious,
    Previous,
    Selected,
    Next,
    FarNext,
    OuterNext,
}

const SHADOW_PAD_X: i32 = 42;
const SHADOW_PAD_TOP: i32 = 10;
const SHADOW_PAD_BOTTOM: i32 = 44;

pub fn image_dimensions(role: ThumbnailRole) -> (i32, i32) {
    match role {
        ThumbnailRole::Selected => (250, 141),
        ThumbnailRole::Previous | ThumbnailRole::Next => (150, 84),
        ThumbnailRole::FarPrevious | ThumbnailRole::FarNext => (108, 61),
        ThumbnailRole::OuterPrevious | ThumbnailRole::OuterNext => (78, 44),
    }
}

pub fn dimensions(role: ThumbnailRole) -> (i32, i32) {
    let (width, height) = image_dimensions(role);
    (
        width + (SHADOW_PAD_X * 2),
        height + SHADOW_PAD_TOP + SHADOW_PAD_BOTTOM,
    )
}

pub fn build(
    item: Option<&WallpaperItem>,
    role: ThumbnailRole,
    active: bool,
    animate_live: bool,
    force_top: bool,
) -> gtk::Box {
    let root = gtk::Box::new(gtk::Orientation::Vertical, 4);
    root.add_css_class("thumb");
    match role {
        ThumbnailRole::Selected => root.add_css_class("selected"),
        ThumbnailRole::Previous | ThumbnailRole::Next => root.add_css_class("neighbor"),
        ThumbnailRole::FarPrevious | ThumbnailRole::FarNext => root.add_css_class("far-neighbor"),
        ThumbnailRole::OuterPrevious | ThumbnailRole::OuterNext => {
            root.add_css_class("outer-neighbor")
        }
    }
    if active {
        root.add_css_class("active");
    }
    if force_top {
        root.add_css_class("top-card");
    }
    let (width, height) = image_dimensions(role);
    let (allocation_width, allocation_height) = dimensions(role);
    root.set_width_request(allocation_width);
    root.set_height_request(allocation_height);

    if let Some(item) = item {
        let overlay = gtk::Overlay::new();
        overlay.set_width_request(width);
        overlay.set_height_request(height);
        overlay.set_margin_start(SHADOW_PAD_X);
        overlay.set_margin_end(SHADOW_PAD_X);
        overlay.set_margin_top(SHADOW_PAD_TOP);
        overlay.set_margin_bottom(SHADOW_PAD_BOTTOM);
        let selected = role == ThumbnailRole::Selected;
        if let Some(display_path) = preview::display_path(item, selected, animate_live) {
            let image = gtk::Picture::for_filename(display_path);
            image.add_css_class("thumb-image");
            if selected {
                image.add_css_class("selected-image");
            }
            image.set_can_shrink(true);
            image.set_content_fit(gtk::ContentFit::Cover);
            image.set_width_request(width);
            image.set_height_request(height);
            overlay.set_child(Some(&image));
        } else {
            let placeholder = gtk::Box::new(gtk::Orientation::Vertical, 0);
            placeholder.add_css_class("thumb-placeholder");
            placeholder.set_width_request(width);
            placeholder.set_height_request(height);
            overlay.set_child(Some(&placeholder));
        }

        if item.is_live() {
            let badge = gtk::Label::new(Some("\u{f0230}"));
            badge.add_css_class("live-badge");
            badge.set_halign(gtk::Align::Start);
            badge.set_valign(gtk::Align::Start);
            badge.set_margin_start(6);
            badge.set_margin_top(5);
            overlay.add_overlay(&badge);
        }

        root.append(&overlay);
    }

    root
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_thumbnail_keeps_sixteen_by_nine_image_ratio() {
        let (width, height) = image_dimensions(ThumbnailRole::Selected);

        assert_eq!((width, height), (250, 141));
        assert!(((width as f64 / height as f64) - (16.0 / 9.0)).abs() < 0.01);
    }

    #[test]
    fn thumbnail_allocation_reserves_shadow_padding() {
        let (image_width, image_height) = image_dimensions(ThumbnailRole::Selected);
        let (allocation_width, allocation_height) = dimensions(ThumbnailRole::Selected);

        assert_eq!(allocation_width - image_width, SHADOW_PAD_X * 2);
        assert_eq!(
            allocation_height - image_height,
            SHADOW_PAD_TOP + SHADOW_PAD_BOTTOM
        );
        assert!(allocation_height > image_height);
    }

    #[test]
    fn all_thumbnail_roles_use_fixed_image_aspect_without_stretching() {
        for role in [
            ThumbnailRole::Selected,
            ThumbnailRole::Previous,
            ThumbnailRole::Next,
            ThumbnailRole::FarPrevious,
            ThumbnailRole::FarNext,
            ThumbnailRole::OuterPrevious,
            ThumbnailRole::OuterNext,
        ] {
            let (width, height) = image_dimensions(role);

            assert!(((width as f64 / height as f64) - (16.0 / 9.0)).abs() < 0.02);
        }
    }
}
