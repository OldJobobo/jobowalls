use crate::shell::cli::{ShellArgs, ShellPosition};
use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

pub fn configure(window: &gtk::ApplicationWindow, args: &ShellArgs) {
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
    window.set_exclusive_zone(0);
    window.set_keyboard_mode(KeyboardMode::OnDemand);
    window.set_anchor(Edge::Bottom, true);
    window.set_margin(Edge::Bottom, 48);

    if args.position == ShellPosition::Center {
        window.set_anchor(Edge::Top, true);
        window.set_margin(Edge::Bottom, 0);
    }
}
