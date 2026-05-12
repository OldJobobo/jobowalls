use crate::shell::{
    cli::ShellLayout,
    model::WallpaperItem,
    widgets::thumbnail::{self, ThumbnailRole},
};
use gtk::prelude::*;
use std::{collections::BTreeSet, path::Path};

pub const STAGE_WIDTH: i32 = 860;
pub const STAGE_HEIGHT: i32 = 204;
pub const VERTICAL_STAGE_WIDTH: i32 = 340;
pub const VERTICAL_STAGE_HEIGHT: i32 = 860;
const SLOT_IMAGE_OVERLAP: f64 = 18.0;

pub fn build(
    items: &[WallpaperItem],
    selected: usize,
    active_wallpaper: Option<&Path>,
    animate_live: bool,
    previous_selected: Option<usize>,
    animation_progress: f64,
    animation_direction: isize,
    layout: ShellLayout,
) -> gtk::Fixed {
    let root = gtk::Fixed::new();
    root.add_css_class("carousel");
    if layout == ShellLayout::Vertical {
        root.add_css_class("vertical");
    }
    root.set_halign(gtk::Align::Center);
    root.set_valign(gtk::Align::Center);
    let (stage_width, stage_height) = stage_dimensions(layout);
    root.set_width_request(stage_width);
    root.set_height_request(stage_height);

    if items.is_empty() {
        return root;
    }

    let len = items.len();
    let selected = selected % len;
    let previous_selected = previous_selected.map(|index| index % len);
    let mut visible = visible_indexes(len, selected);
    if let Some(previous_selected) = previous_selected {
        visible.extend(visible_indexes(len, previous_selected));
    }

    let mut cards = Vec::new();
    for index in visible {
        let target_slot = slot_for_index(len, selected, index);
        let start_slot =
            previous_selected.and_then(|previous| slot_for_index(len, previous, index));
        let slot = interpolated_slot(
            start_slot,
            target_slot,
            animation_progress,
            animation_direction,
        );
        cards.push((index, slot, index == selected));
    }

    cards.sort_by(|(_, a, selected_a), (_, b, selected_b)| {
        b.abs()
            .total_cmp(&a.abs())
            .then_with(|| selected_a.cmp(selected_b))
    });

    for (index, slot, is_selected) in cards {
        let role = role_for_slot(slot);
        let (image_width, image_height) = image_dimensions_for_slot(slot);
        let (width, height) = allocation_dimensions(image_width, image_height);
        let card = thumbnail::build_with_image_dimensions(
            Some(&items[index]),
            role,
            is_active(items, Some(index), active_wallpaper),
            animate_live,
            is_selected,
            image_width,
            image_height,
        );
        card.set_opacity(opacity_for_slot(slot, is_selected));
        let (x, y) = card_position(slot, width, height, layout);
        root.put(&card, x, y);
    }

    root
}

pub fn stage_dimensions(layout: ShellLayout) -> (i32, i32) {
    match layout {
        ShellLayout::Horizontal => (STAGE_WIDTH, STAGE_HEIGHT),
        ShellLayout::Vertical => (VERTICAL_STAGE_WIDTH, VERTICAL_STAGE_HEIGHT),
    }
}

pub fn debug_geometry(
    items: &[WallpaperItem],
    selected: usize,
    active_wallpaper: Option<&Path>,
    animate_live: bool,
    previous_selected: Option<usize>,
    animation_progress: f64,
    animation_direction: isize,
    layout: ShellLayout,
) -> Vec<String> {
    let _ = (active_wallpaper, animate_live);
    if items.is_empty() {
        return Vec::new();
    }

    let len = items.len();
    let selected = selected % len;
    let previous_selected = previous_selected.map(|index| index % len);
    let mut visible = visible_indexes(len, selected);
    if let Some(previous_selected) = previous_selected {
        visible.extend(visible_indexes(len, previous_selected));
    }

    visible
        .into_iter()
        .map(|index| {
            let target_slot = slot_for_index(len, selected, index);
            let start_slot =
                previous_selected.and_then(|previous| slot_for_index(len, previous, index));
            let slot = interpolated_slot(
                start_slot,
                target_slot,
                animation_progress,
                animation_direction,
            );
            let role = role_for_slot(slot);
            let (image_width, image_height) = image_dimensions_for_slot(slot);
            let (width, height) = allocation_dimensions(image_width, image_height);
            let (x, y) = card_position(slot, width, height, layout);
            format!(
                "  card index={index} selected={} start={start_slot:?} target={target_slot:?} slot={slot:.3} role={role:?} image={}x{} alloc={}x{} xy={x:.1},{y:.1}",
                index == selected,
                image_width,
                image_height,
                width,
                height,
            )
        })
        .collect()
}

fn visible_indexes(len: usize, selected: usize) -> BTreeSet<usize> {
    (-3..=3)
        .filter_map(|offset| offset_index(len, selected, offset))
        .collect()
}

fn offset_index(len: usize, selected: usize, offset: isize) -> Option<usize> {
    if offset == 0 && len > 0 {
        return Some(selected % len);
    }

    if len < 2 {
        return None;
    }

    let distance = offset.unsigned_abs();
    if distance >= len {
        return None;
    }

    let index = if offset < 0 {
        (selected + len - distance) % len
    } else {
        (selected + distance) % len
    };
    Some(index)
}

fn slot_for_index(len: usize, selected: usize, index: usize) -> Option<isize> {
    (-3..=3).find(|offset| offset_index(len, selected, *offset) == Some(index))
}

fn interpolated_slot(
    start_slot: Option<isize>,
    target_slot: Option<isize>,
    progress: f64,
    direction: isize,
) -> f64 {
    let Some(target_slot) = target_slot else {
        return f64::from((start_slot.unwrap_or_default() - direction) as i32);
    };
    let start_slot = start_slot.unwrap_or(target_slot + direction);
    f64::from(start_slot as i32)
        + (f64::from(target_slot as i32) - f64::from(start_slot as i32)) * progress
}

fn role_for_slot(slot: f64) -> ThumbnailRole {
    let rounded = slot.round() as isize;
    if rounded == 0 {
        ThumbnailRole::Selected
    } else if rounded == -1 {
        ThumbnailRole::Previous
    } else if rounded == 1 {
        ThumbnailRole::Next
    } else if rounded == -2 {
        ThumbnailRole::FarPrevious
    } else if rounded == 2 {
        ThumbnailRole::FarNext
    } else if rounded < 0 {
        ThumbnailRole::OuterPrevious
    } else {
        ThumbnailRole::OuterNext
    }
}

fn image_dimensions_for_slot(slot: f64) -> (i32, i32) {
    const SIZE_STOPS: [(f64, ThumbnailRole); 4] = [
        (0.0, ThumbnailRole::Selected),
        (1.0, ThumbnailRole::Previous),
        (2.0, ThumbnailRole::FarPrevious),
        (3.0, ThumbnailRole::OuterPrevious),
    ];

    let distance = slot.abs().clamp(0.0, 3.0);
    let lower = distance.floor() as usize;
    let upper = distance.ceil() as usize;
    let (_, lower_role) = SIZE_STOPS[lower];
    let (_, upper_role) = SIZE_STOPS[upper];
    let (lower_width, lower_height) = thumbnail::image_dimensions(lower_role);
    let (upper_width, upper_height) = thumbnail::image_dimensions(upper_role);
    let progress = distance - distance.floor();

    (
        lerp_i32(lower_width, upper_width, progress),
        lerp_i32(lower_height, upper_height, progress),
    )
}

fn allocation_dimensions(image_width: i32, image_height: i32) -> (i32, i32) {
    let (selected_image_width, selected_image_height) =
        thumbnail::image_dimensions(ThumbnailRole::Selected);
    let (selected_width, selected_height) = thumbnail::dimensions(ThumbnailRole::Selected);
    (
        image_width + (selected_width - selected_image_width),
        image_height + (selected_height - selected_image_height),
    )
}

fn lerp_i32(start: i32, end: i32, progress: f64) -> i32 {
    (f64::from(start) + (f64::from(end - start) * progress)).round() as i32
}

fn card_position(slot: f64, width: i32, height: i32, layout: ShellLayout) -> (f64, f64) {
    let (stage_width, stage_height) = stage_dimensions(layout);
    match layout {
        ShellLayout::Horizontal => (
            slot_center(slot, layout) - f64::from(width) / 2.0,
            f64::from(stage_height - height) / 2.0,
        ),
        ShellLayout::Vertical => (
            f64::from(stage_width - width) / 2.0,
            slot_center(slot, layout) - f64::from(height) / 2.0,
        ),
    }
}

fn slot_center(slot: f64, layout: ShellLayout) -> f64 {
    let lower = slot.floor().clamp(-3.0, 3.0) as isize;
    let upper = slot.ceil().clamp(-3.0, 3.0) as isize;
    let lower_center = center_for_slot(lower, layout);
    let upper_center = center_for_slot(upper, layout);
    if lower == upper {
        lower_center
    } else {
        lower_center + (upper_center - lower_center) * (slot - slot.floor())
    }
}

fn center_for_slot(slot: isize, layout: ShellLayout) -> f64 {
    let slot = slot.clamp(-3, 3);
    let selected_center = f64::from(main_axis_extent(layout)) / 2.0;
    match slot {
        -3 => selected_center - center_distance_between_slots(-3, 0, layout),
        -2 => selected_center - center_distance_between_slots(-2, 0, layout),
        -1 => selected_center - center_distance_between_slots(-1, 0, layout),
        0 => selected_center,
        1 => selected_center + center_distance_between_slots(0, 1, layout),
        2 => selected_center + center_distance_between_slots(0, 2, layout),
        3 => selected_center + center_distance_between_slots(0, 3, layout),
        _ => selected_center,
    }
}

fn center_distance_between_slots(start: isize, end: isize, layout: ShellLayout) -> f64 {
    if start == end {
        return 0.0;
    }

    let mut distance = 0.0;
    for slot in start..end {
        let start_size = main_axis_image_size_for_resting_slot(slot, layout);
        let end_size = main_axis_image_size_for_resting_slot(slot + 1, layout);
        distance += ((start_size + end_size) / 2.0) - SLOT_IMAGE_OVERLAP;
    }
    distance
}

#[cfg(test)]
fn image_width_for_resting_slot(slot: isize) -> f64 {
    main_axis_image_size_for_resting_slot(slot, ShellLayout::Horizontal)
}

fn main_axis_image_size_for_resting_slot(slot: isize, layout: ShellLayout) -> f64 {
    let role = role_for_slot(slot as f64);
    let (width, height) = thumbnail::image_dimensions(role);
    match layout {
        ShellLayout::Horizontal => f64::from(width),
        ShellLayout::Vertical => f64::from(height),
    }
}

fn main_axis_extent(layout: ShellLayout) -> i32 {
    let (width, height) = stage_dimensions(layout);
    match layout {
        ShellLayout::Horizontal => width,
        ShellLayout::Vertical => height,
    }
}

fn opacity_for_slot(slot: f64, is_selected: bool) -> f64 {
    if is_selected {
        return 1.0;
    }

    let distance = slot.abs();
    if distance < 0.5 {
        1.0
    } else if distance < 1.5 {
        0.76
    } else if distance < 2.5 {
        0.52
    } else if distance < 3.5 {
        0.34
    } else {
        0.22
    }
}

#[cfg(test)]
fn centered_y_for_role(role: ThumbnailRole) -> f64 {
    let (_, height) = thumbnail::dimensions(role);
    f64::from(STAGE_HEIGHT - height) / 2.0
}

#[cfg(test)]
fn centered_x_for_role(role: ThumbnailRole) -> f64 {
    let (width, _) = thumbnail::dimensions(role);
    f64::from(VERTICAL_STAGE_WIDTH - width) / 2.0
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_is_tall_enough_for_selected_card_shadow_allocation() {
        let (_, selected_height) = thumbnail::dimensions(ThumbnailRole::Selected);

        assert!(STAGE_HEIGHT >= selected_height);
    }

    #[test]
    fn vertical_stage_is_wide_enough_for_selected_card_shadow_allocation() {
        let (selected_width, _) = thumbnail::dimensions(ThumbnailRole::Selected);

        assert!(VERTICAL_STAGE_WIDTH >= selected_width);
    }

    #[test]
    fn visible_indexes_wrap_around_selected_item() {
        let visible = visible_indexes(10, 0);

        assert_eq!(visible, BTreeSet::from([0, 1, 2, 3, 7, 8, 9]));
    }

    #[test]
    fn visible_indexes_avoid_duplicate_far_cards_for_small_sets() {
        let visible = visible_indexes(3, 1);

        assert_eq!(visible, BTreeSet::from([0, 1, 2]));
    }

    #[test]
    fn visible_indexes_include_seven_cards_when_available() {
        let visible = visible_indexes(12, 6);

        assert_eq!(visible.len(), 7);
        assert_eq!(visible, BTreeSet::from([3, 4, 5, 6, 7, 8, 9]));
    }

    #[test]
    fn interpolation_moves_cards_smoothly_between_slots() {
        let slot = interpolated_slot(Some(-1), Some(0), 0.25, 1);

        assert_eq!(slot, -0.75);
    }

    #[test]
    fn slot_center_interpolates_between_neighbor_centers() {
        let center = slot_center(-0.5, ShellLayout::Horizontal);

        assert_eq!(center, 339.0);
    }

    #[test]
    fn resting_slot_images_overlap_evenly() {
        for slot in -3..3 {
            let left_width = image_width_for_resting_slot(slot);
            let right_width = image_width_for_resting_slot(slot + 1);
            let left_right_edge = center_for_slot(slot, ShellLayout::Horizontal) + left_width / 2.0;
            let right_left_edge =
                center_for_slot(slot + 1, ShellLayout::Horizontal) - right_width / 2.0;

            assert_eq!(left_right_edge - right_left_edge, SLOT_IMAGE_OVERLAP);
        }
    }

    #[test]
    fn vertical_resting_slot_images_overlap_evenly() {
        for slot in -3..3 {
            let top_height = main_axis_image_size_for_resting_slot(slot, ShellLayout::Vertical);
            let bottom_height =
                main_axis_image_size_for_resting_slot(slot + 1, ShellLayout::Vertical);
            let top_bottom_edge = center_for_slot(slot, ShellLayout::Vertical) + top_height / 2.0;
            let bottom_top_edge =
                center_for_slot(slot + 1, ShellLayout::Vertical) - bottom_height / 2.0;

            assert_eq!(top_bottom_edge - bottom_top_edge, SLOT_IMAGE_OVERLAP);
        }
    }

    #[test]
    fn image_dimensions_interpolate_between_selected_and_neighbor() {
        let dimensions = image_dimensions_for_slot(0.5);

        assert_eq!(dimensions, (200, 113));
    }

    #[test]
    fn selected_card_is_always_fully_opaque() {
        assert_eq!(opacity_for_slot(2.0, true), 1.0);
    }

    #[test]
    fn outer_slots_use_outer_thumbnail_roles() {
        assert_eq!(role_for_slot(-3.0), ThumbnailRole::OuterPrevious);
        assert_eq!(role_for_slot(3.0), ThumbnailRole::OuterNext);
    }

    #[test]
    fn non_selected_cards_are_vertically_centered_in_stage() {
        assert!(centered_y_for_role(ThumbnailRole::Previous) > 0.0);
        assert!(centered_y_for_role(ThumbnailRole::FarPrevious) > 0.0);
        assert!(centered_y_for_role(ThumbnailRole::OuterPrevious) > 0.0);
        assert_eq!(centered_y_for_role(ThumbnailRole::Previous), 33.0);
    }

    #[test]
    fn vertical_cards_are_horizontally_centered_in_stage() {
        assert!(centered_x_for_role(ThumbnailRole::Previous) > 0.0);
        assert!(centered_x_for_role(ThumbnailRole::FarPrevious) > 0.0);
        assert!(centered_x_for_role(ThumbnailRole::OuterPrevious) > 0.0);
        assert_eq!(centered_x_for_role(ThumbnailRole::Previous), 53.0);
    }
}
