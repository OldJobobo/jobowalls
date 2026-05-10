use crate::shell::{
    model::WallpaperItem,
    widgets::thumbnail::{self, ThumbnailRole},
};
use gtk::prelude::*;
use std::{collections::BTreeSet, path::Path};

pub const STAGE_WIDTH: i32 = 860;
const STAGE_HEIGHT: i32 = 204;
const SLOT_CENTERS: [(isize, f64); 7] = [
    (-3, 42.0),
    (-2, 155.0),
    (-1, 320.0),
    (0, 430.0),
    (1, 540.0),
    (2, 705.0),
    (3, 818.0),
];

pub fn build(
    items: &[WallpaperItem],
    selected: usize,
    active_wallpaper: Option<&Path>,
    animate_live: bool,
    previous_selected: Option<usize>,
    animation_progress: f64,
    animation_direction: isize,
) -> gtk::Fixed {
    let root = gtk::Fixed::new();
    root.add_css_class("carousel");
    root.set_halign(gtk::Align::Center);
    root.set_valign(gtk::Align::Center);
    root.set_width_request(STAGE_WIDTH);
    root.set_height_request(STAGE_HEIGHT);

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
        cards.push((index, slot));
    }

    cards.sort_by(|(_, a), (_, b)| b.abs().total_cmp(&a.abs()));

    for (index, slot) in cards {
        let role = role_for_slot(slot);
        let (width, _) = thumbnail::dimensions(role);
        let card = thumbnail::build(
            Some(&items[index]),
            role,
            is_active(items, Some(index), active_wallpaper),
            animate_live,
            index == selected,
        );
        card.set_opacity(opacity_for_slot(slot, index == selected));
        let x = slot_center(slot) - f64::from(width) / 2.0;
        let y = centered_y_for_role(role);
        root.put(&card, x, y);
    }

    root
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

fn slot_center(slot: f64) -> f64 {
    let lower = slot.floor().clamp(-3.0, 3.0) as isize;
    let upper = slot.ceil().clamp(-3.0, 3.0) as isize;
    let lower_center = center_for_slot(lower);
    let upper_center = center_for_slot(upper);
    if lower == upper {
        lower_center
    } else {
        lower_center + (upper_center - lower_center) * (slot - slot.floor())
    }
}

fn center_for_slot(slot: isize) -> f64 {
    SLOT_CENTERS
        .iter()
        .find_map(|(candidate, center)| (*candidate == slot).then_some(*center))
        .unwrap_or(380.0)
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

fn centered_y_for_role(role: ThumbnailRole) -> f64 {
    let (_, height) = thumbnail::dimensions(role);
    f64::from(STAGE_HEIGHT - height) / 2.0
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
        let center = slot_center(-0.5);

        assert_eq!(center, 375.0);
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
}
