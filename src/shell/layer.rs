use crate::shell::cli::{ShellArgs, ShellPosition};
use gtk::gdk;
use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

const EDGE_POSITION_INSET_PX: i32 = 55;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AnchorSpec {
    top: bool,
    bottom: bool,
    left: bool,
    right: bool,
}

fn panel_anchor_spec(position: ShellPosition) -> AnchorSpec {
    match position {
        ShellPosition::Top => AnchorSpec {
            top: true,
            bottom: false,
            left: true,
            right: false,
        },
        ShellPosition::Bottom => AnchorSpec {
            top: false,
            bottom: true,
            left: true,
            right: false,
        },
        ShellPosition::Center => AnchorSpec {
            top: true,
            bottom: false,
            left: true,
            right: false,
        },
        ShellPosition::Left => AnchorSpec {
            top: true,
            bottom: false,
            left: true,
            right: false,
        },
        ShellPosition::Right => AnchorSpec {
            top: true,
            bottom: false,
            left: false,
            right: true,
        },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceDimensions {
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EdgeMargins {
    top: i32,
    bottom: i32,
    left: i32,
    right: i32,
}

fn panel_margins(
    position: ShellPosition,
    panel: SurfaceDimensions,
    output: SurfaceDimensions,
) -> EdgeMargins {
    let center_x = ((output.width - panel.width).max(0)) / 2;
    let center_y = ((output.height - panel.height).max(0)) / 2;

    match position {
        ShellPosition::Top => EdgeMargins {
            top: EDGE_POSITION_INSET_PX,
            bottom: 0,
            left: center_x,
            right: 0,
        },
        ShellPosition::Bottom => EdgeMargins {
            top: 0,
            bottom: 0,
            left: center_x,
            right: 0,
        },
        ShellPosition::Center => EdgeMargins {
            top: center_y,
            bottom: 0,
            left: center_x,
            right: 0,
        },
        ShellPosition::Left => EdgeMargins {
            top: center_y,
            bottom: 0,
            left: EDGE_POSITION_INSET_PX,
            right: 0,
        },
        ShellPosition::Right => EdgeMargins {
            top: center_y,
            bottom: 0,
            left: 0,
            right: EDGE_POSITION_INSET_PX,
        },
    }
}

fn dismiss_anchor_spec() -> AnchorSpec {
    AnchorSpec {
        top: true,
        bottom: true,
        left: true,
        right: true,
    }
}

pub fn configure_panel(
    window: &gtk::ApplicationWindow,
    args: &ShellArgs,
    monitor: Option<&gdk::Monitor>,
    panel: SurfaceDimensions,
    output: SurfaceDimensions,
) {
    window.set_decorated(args.debug_window);

    if args.debug_window {
        return;
    }

    if !gtk4_layer_shell::is_supported() {
        window.set_decorated(false);
        return;
    }

    window.init_layer_shell();
    window.set_namespace(Some("jobowalls-shell"));
    window.set_layer(Layer::Overlay);
    window.set_monitor(monitor);
    window.set_exclusive_zone(0);
    window.set_keyboard_mode(KeyboardMode::Exclusive);
    apply_anchors(window, panel_anchor_spec(args.position()));
    apply_margins(window, panel_margins(args.position(), panel, output));
}

pub fn configure_click_catcher(window: &gtk::ApplicationWindow, monitor: Option<&gdk::Monitor>) {
    window.set_decorated(false);

    if !gtk4_layer_shell::is_supported() {
        window.set_decorated(false);
        return;
    }

    window.init_layer_shell();
    window.set_namespace(Some("jobowalls-shell-dismiss"));
    window.set_layer(Layer::Top);
    window.set_monitor(monitor);
    window.set_exclusive_zone(0);
    window.set_keyboard_mode(KeyboardMode::None);
    apply_anchors(window, dismiss_anchor_spec());
    reset_margins(window);
}

fn apply_anchors(window: &gtk::ApplicationWindow, anchors: AnchorSpec) {
    window.set_anchor(Edge::Top, anchors.top);
    window.set_anchor(Edge::Bottom, anchors.bottom);
    window.set_anchor(Edge::Left, anchors.left);
    window.set_anchor(Edge::Right, anchors.right);
}

fn reset_margins(window: &gtk::ApplicationWindow) {
    apply_margins(
        window,
        EdgeMargins {
            top: 0,
            bottom: 0,
            left: 0,
            right: 0,
        },
    );
}

fn apply_margins(window: &gtk::ApplicationWindow, margins: EdgeMargins) {
    window.set_margin(Edge::Top, margins.top);
    window.set_margin(Edge::Bottom, margins.bottom);
    window.set_margin(Edge::Left, margins.left);
    window.set_margin(Edge::Right, margins.right);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bottom_panel_uses_layer_shell_bottom_center_placement() {
        assert_eq!(
            panel_anchor_spec(ShellPosition::Bottom),
            AnchorSpec {
                top: false,
                bottom: true,
                left: true,
                right: false,
            }
        );
    }

    #[test]
    fn left_panel_uses_layer_shell_left_center_placement() {
        assert_eq!(
            panel_anchor_spec(ShellPosition::Left),
            AnchorSpec {
                top: true,
                bottom: false,
                left: true,
                right: false,
            }
        );
    }

    #[test]
    fn top_panel_uses_layer_shell_top_center_placement() {
        assert_eq!(
            panel_anchor_spec(ShellPosition::Top),
            AnchorSpec {
                top: true,
                bottom: false,
                left: true,
                right: false,
            }
        );
    }

    #[test]
    fn right_panel_uses_layer_shell_right_center_placement() {
        assert_eq!(
            panel_anchor_spec(ShellPosition::Right),
            AnchorSpec {
                top: true,
                bottom: false,
                left: false,
                right: true,
            }
        );
    }

    #[test]
    fn bottom_panel_centers_with_left_margin_without_stretching() {
        assert_eq!(
            panel_margins(
                ShellPosition::Bottom,
                SurfaceDimensions {
                    width: 860,
                    height: 340,
                },
                SurfaceDimensions {
                    width: 2560,
                    height: 1440,
                },
            ),
            EdgeMargins {
                top: 0,
                bottom: 0,
                left: 850,
                right: 0,
            }
        );
    }

    #[test]
    fn left_panel_centers_with_top_margin_without_stretching() {
        assert_eq!(
            panel_margins(
                ShellPosition::Left,
                SurfaceDimensions {
                    width: 340,
                    height: 886,
                },
                SurfaceDimensions {
                    width: 2560,
                    height: 1440,
                },
            ),
            EdgeMargins {
                top: 277,
                bottom: 0,
                left: EDGE_POSITION_INSET_PX,
                right: 0,
            }
        );
    }

    #[test]
    fn right_panel_is_inset_from_edge_to_mirror_left_panel() {
        assert_eq!(
            panel_margins(
                ShellPosition::Right,
                SurfaceDimensions {
                    width: 340,
                    height: 886,
                },
                SurfaceDimensions {
                    width: 2560,
                    height: 1440,
                },
            ),
            EdgeMargins {
                top: 277,
                bottom: 0,
                left: 0,
                right: EDGE_POSITION_INSET_PX,
            }
        );
    }

    #[test]
    fn top_panel_is_inset_from_edge_to_match_bottom_visual_weight() {
        assert_eq!(
            panel_margins(
                ShellPosition::Top,
                SurfaceDimensions {
                    width: 860,
                    height: 340,
                },
                SurfaceDimensions {
                    width: 2560,
                    height: 1440,
                },
            ),
            EdgeMargins {
                top: EDGE_POSITION_INSET_PX,
                bottom: 0,
                left: 850,
                right: 0,
            }
        );
    }

    #[test]
    fn dismiss_surface_is_fullscreen_but_separate_from_panel() {
        assert_eq!(
            dismiss_anchor_spec(),
            AnchorSpec {
                top: true,
                bottom: true,
                left: true,
                right: true,
            }
        );
    }
}
