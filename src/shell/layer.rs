use crate::shell::cli::{ShellArgs, ShellPosition};
use gtk::gdk;
use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AnchorSpec {
    top: bool,
    bottom: bool,
    left: bool,
    right: bool,
}

fn panel_anchor_spec(position: ShellPosition) -> AnchorSpec {
    AnchorSpec {
        top: position == ShellPosition::Center,
        bottom: true,
        left: false,
        right: false,
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
    let anchors = panel_anchor_spec(args.position());
    apply_anchors(window, anchors);
    window.set_margin(Edge::Bottom, 0);

    if args.position() == ShellPosition::Center {
        window.set_margin(Edge::Bottom, 0);
    }
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
}

fn apply_anchors(window: &gtk::ApplicationWindow, anchors: AnchorSpec) {
    window.set_anchor(Edge::Top, anchors.top);
    window.set_anchor(Edge::Bottom, anchors.bottom);
    window.set_anchor(Edge::Left, anchors.left);
    window.set_anchor(Edge::Right, anchors.right);
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
                left: false,
                right: false,
            }
        );
    }

    #[test]
    fn center_panel_uses_layer_shell_center_placement() {
        assert_eq!(
            panel_anchor_spec(ShellPosition::Center),
            AnchorSpec {
                top: true,
                bottom: true,
                left: false,
                right: false,
            }
        );
    }

    #[test]
    fn dismiss_surface_is_full_screen_but_separate_from_panel() {
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
