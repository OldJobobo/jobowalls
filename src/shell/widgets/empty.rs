use gtk::prelude::*;

pub fn build(message: &str) -> gtk::Box {
    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    root.add_css_class("empty");
    root.set_halign(gtk::Align::Center);
    root.set_valign(gtk::Align::Center);

    let label = gtk::Label::new(Some(message));
    label.set_wrap(true);
    root.append(&label);
    root
}
