//! Application identity from package metadata and shared transparent artwork.
use super::*;

impl Preferences {
    pub(super) fn build_about(&self) {
        let page = self.page("about", "About", "help-about-symbolic");
        let group = self.group(&page, "", "");
        let card = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(28)
            .margin_top(24)
            .margin_bottom(24)
            .css_classes(["clivuu-about-card"])
            .build();
        let identity = gtk::Box::new(gtk::Orientation::Vertical, 12);
        identity.set_margin_top(12);
        identity.set_margin_bottom(8);
        // File icons are rasterized at the requested size and surface scale;
        // from_file would enlarge the SVG's 64px intrinsic raster here.
        let icon = gtk::Image::from_gicon(&gtk::gio::FileIcon::new(&gtk::gio::File::for_path(
            crate::platform::integration::icons::transparent(),
        )));
        icon.set_pixel_size(112);
        icon.set_margin_bottom(8);
        icon.set_widget_name("about-icon");
        identity.append(&icon);
        let name = gtk::Label::new(Some("Clivuu"));
        name.add_css_class("clivuu-about-name");
        identity.append(&name);
        let version = gtk::Label::new(Some(env!("CARGO_PKG_VERSION")));
        version.set_widget_name("about-version");
        version.set_halign(gtk::Align::Center);
        version.add_css_class("clivuu-about-version");
        identity.append(&version);
        card.append(&identity);
        let project = gtk::LinkButton::with_label("https://github.com/Naerelyth/clivuu", "GitHub");
        project.set_widget_name("about-project");
        project.add_css_class("flat");
        project.add_css_class("clivuu-about-project");
        let action = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let github = gtk::Image::from_gicon(&gtk::gio::FileIcon::new(&gtk::gio::File::for_path(
            crate::platform::integration::icons::github(),
        )));
        github.set_widget_name("about-github-icon");
        action.append(&github);
        let label = gtk::Label::builder().xalign(0.).hexpand(true).build();
        self.text(&label, "label", "View source on GitHub");
        action.append(&label);
        action.append(&gtk::Image::from_icon_name("adw-external-link-symbolic"));
        project.set_child(Some(&action));
        card.append(&project);
        group.add(&card);
    }
}
