mod audio;
mod noise;
mod settings;
mod user_presets;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use gtk::prelude::*;
use gtk::{
    glib, Align, Application, ApplicationWindow, Box as GtkBox, Button, Entry, Label, ListBox,
    MenuButton, Orientation, Popover, PositionType, Scale, ScrolledWindow,
};

use audio::Controls;
use noise::{Preset, BAND_COUNT, MAX_DB, MIN_DB};
use settings::Settings;
use user_presets::PresetLibrary;

const APP_ID: &str = "us.jlong.NoiseGenerator";

fn main() -> glib::ExitCode {
    let controls = Controls::new();

    // Apply saved settings before audio starts, so the first sound is already right.
    let settings = Settings::load();
    for (band, db) in settings.band_db.iter().enumerate() {
        controls.set_band_db(band, *db);
    }
    controls.set_volume(settings.volume);

    // The stream must stay alive for the whole program, so it lives here in
    // main (cpal streams are not Send, so it can't move into GTK callbacks freely).
    let _stream = match audio::start(controls.clone()) {
        Ok(stream) => Some(stream),
        Err(err) => {
            eprintln!("failed to start audio: {err}");
            None
        }
    };

    let app = Application::builder().application_id(APP_ID).build();
    app.connect_activate(move |app| build_ui(app, controls.clone(), &settings));
    app.run()
}

fn build_ui(app: &Application, controls: Arc<Controls>, settings: &Settings) {
    let root = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(18)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(24)
        .margin_end(24)
        .build();

    let title = Label::builder().label("Noise Generator").css_classes(["title-1"]).build();
    root.append(&title);

    // Equalizer: one vertical slider per octave band.
    let eq_row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(6)
        .homogeneous(true)
        .build();
    let sliders: Vec<Scale> = (0..BAND_COUNT)
        .map(|band| {
            let scale = Scale::with_range(Orientation::Vertical, MIN_DB as f64, MAX_DB as f64, 1.0);
            // Vertical scales put the minimum at the top by default.
            scale.set_inverted(true);
            scale.set_value(settings.band_db[band] as f64);
            scale.set_digits(0);
            scale.set_draw_value(true);
            scale.set_value_pos(PositionType::Top);
            scale.set_height_request(220);
            scale.add_mark(0.0, PositionType::Right, None);
            let controls = controls.clone();
            scale.connect_value_changed(move |s| controls.set_band_db(band, s.value() as f32));

            let column = GtkBox::builder().orientation(Orientation::Vertical).spacing(4).build();
            column.append(&scale);
            column.append(&Label::builder().label(noise::band_label(band)).css_classes(["caption"]).build());
            eq_row.append(&column);
            scale
        })
        .collect();
    root.append(&eq_row);

    let axis_hint = Label::builder()
        .label("Band levels in dB (Hz below). Levels are relative; loudness is set by Volume.")
        .wrap(true)
        .css_classes(["dim-label", "caption"])
        .build();
    root.append(&axis_hint);

    // Presets: built-in noise colors, plus the user's own saved presets.
    let preset_row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .halign(Align::Center)
        .spacing(12)
        .build();
    let builtin_presets = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .css_classes(["linked"])
        .build();
    for preset in Preset::ALL {
        let button = Button::builder()
            .label(preset.label())
            .tooltip_text(preset.description())
            .build();
        let sliders = sliders.clone();
        button.connect_clicked(move |_| {
            for (slider, db) in sliders.iter().zip(preset.band_db()) {
                slider.set_value(db as f64);
            }
        });
        builtin_presets.append(&button);
    }
    preset_row.append(&builtin_presets);
    preset_row.append(&PresetMenu::build(sliders.clone()));
    root.append(&preset_row);

    // Volume
    let volume_row = GtkBox::builder().orientation(Orientation::Horizontal).spacing(12).build();
    volume_row.append(&Label::new(Some("Volume")));
    let volume = Scale::with_range(Orientation::Horizontal, 0.0, 1.0, 0.01);
    volume.set_value(settings.volume as f64);
    volume.set_hexpand(true);
    volume.set_draw_value(false);
    {
        let controls = controls.clone();
        volume.connect_value_changed(move |s| controls.set_volume(s.value() as f32));
    }
    volume_row.append(&volume);
    root.append(&volume_row);

    // Play / pause
    let play = Button::builder()
        .label("Play")
        .halign(Align::Center)
        .width_request(140)
        .css_classes(["suggested-action", "pill"])
        .build();
    {
        let controls = controls.clone();
        let playing = Cell::new(false);
        play.connect_clicked(move |b| {
            let playing = !playing.replace(!playing.get());
            controls.set_playing(playing);
            b.set_label(if playing { "Pause" } else { "Play" });
        });
    }
    root.append(&play);

    let window = ApplicationWindow::builder()
        .application(app)
        .title("Noise Generator")
        .resizable(false)
        .child(&root)
        .build();
    window.connect_close_request(move |_| {
        let settings = Settings {
            band_db: std::array::from_fn(|band| sliders[band].value() as f32),
            volume: volume.value() as f32,
        };
        if let Err(err) = settings.save() {
            eprintln!("failed to save settings: {err}");
        }
        glib::Propagation::Proceed
    });
    window.present();
}

/// The "My Presets" popover: a name entry with Save, and a list of saved
/// presets that load on click and can be deleted.
struct PresetMenu {
    library: PresetLibrary,
    sliders: Vec<Scale>,
    popover: Popover,
    entry: Entry,
    save: Button,
    list: ListBox,
    /// The preset whose row is currently being renamed, if any.
    editing: RefCell<Option<String>>,
}

impl PresetMenu {
    fn build(sliders: Vec<Scale>) -> MenuButton {
        let entry = Entry::builder().placeholder_text("Preset name").hexpand(true).build();
        let save = Button::builder().label("Save").css_classes(["suggested-action"]).build();
        let save_row = GtkBox::builder()
            .orientation(Orientation::Horizontal)
            .css_classes(["linked"])
            .build();
        save_row.append(&entry);
        save_row.append(&save);

        let list = ListBox::builder().selection_mode(gtk::SelectionMode::None).build();
        list.set_placeholder(Some(
            &Label::builder()
                .label("No saved presets yet")
                .css_classes(["dim-label"])
                .margin_top(12)
                .margin_bottom(12)
                .build(),
        ));
        let scroller = ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .max_content_height(260)
            .propagate_natural_height(true)
            .child(&list)
            .build();

        let content = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .spacing(8)
            .width_request(260)
            .build();
        content.append(&save_row);
        content.append(&gtk::Separator::new(Orientation::Horizontal));
        content.append(&scroller);

        let popover = Popover::builder().child(&content).build();
        let menu = Rc::new(PresetMenu {
            library: PresetLibrary::open(),
            sliders,
            popover: popover.clone(),
            entry: entry.clone(),
            save: save.clone(),
            list,
            editing: RefCell::new(None),
        });

        // These closures and the widgets form an Rc cycle, which is fine:
        // the menu should live exactly as long as the window does.
        let m = menu.clone();
        save.connect_clicked(move |_| m.save_current());
        let m = menu.clone();
        entry.connect_activate(move |_| m.save_current());
        let m = menu.clone();
        entry.connect_changed(move |_| m.update_save_button());
        let m = menu.clone();
        popover.connect_show(move |_| {
            // Drop any rename left half-finished when the popover last closed.
            m.editing.replace(None);
            m.refresh();
            m.entry.grab_focus();
        });
        menu.refresh();

        MenuButton::builder()
            .label("My Presets")
            .popover(&popover)
            .tooltip_text("Save the current EQ as a preset, or load one")
            .build()
    }

    fn name(&self) -> String {
        self.entry.text().trim().to_string()
    }

    fn update_save_button(&self) {
        let name = self.name();
        self.save.set_sensitive(user_presets::is_valid_name(&name));
        self.save.set_label(if self.library.get(&name).is_some() { "Overwrite" } else { "Save" });
    }

    fn save_current(self: &Rc<Self>) {
        let name = self.name();
        if !user_presets::is_valid_name(&name) {
            return;
        }
        let band_db = std::array::from_fn(|band| self.sliders[band].value() as f32);
        if let Err(err) = self.library.save(&name, &band_db) {
            eprintln!("failed to save preset {name:?}: {err}");
        }
        self.refresh();
    }

    fn load(&self, name: &str) {
        if let Some(band_db) = self.library.get(name) {
            for (slider, db) in self.sliders.iter().zip(band_db) {
                slider.set_value(db as f64);
            }
            // Prefill the name so tweaking and saving again overwrites it.
            self.entry.set_text(name);
            self.popover.popdown();
        }
    }

    fn delete(self: &Rc<Self>, name: &str) {
        if let Err(err) = self.library.delete(name) {
            eprintln!("failed to delete preset {name:?}: {err}");
        }
        self.refresh();
    }

    fn start_rename(self: &Rc<Self>, name: &str) {
        self.editing.replace(Some(name.to_string()));
        self.refresh();
    }

    fn cancel_rename(self: &Rc<Self>) {
        self.editing.replace(None);
        self.refresh();
    }

    fn commit_rename(self: &Rc<Self>, old: &str, field: &Entry, error: &Label) {
        let new = field.text().trim().to_string();
        match self.library.rename(old, &new) {
            Ok(()) => {
                // Keep the save field pointing at the preset under its new name.
                if self.name() == old {
                    self.entry.set_text(&new);
                }
                self.cancel_rename();
            }
            Err(err) => {
                field.add_css_class("error");
                error.set_label(&err.to_string());
                error.set_visible(true);
            }
        }
    }

    fn refresh(self: &Rc<Self>) {
        self.list.remove_all();
        let editing = self.editing.borrow().clone();
        for name in self.library.names() {
            if editing.as_deref() == Some(name.as_str()) {
                self.append_rename_row(name);
            } else {
                self.append_preset_row(name);
            }
        }
        self.update_save_button();
    }

    fn append_preset_row(self: &Rc<Self>, name: String) {
        let load = Button::builder()
            .child(
                &Label::builder()
                    .label(&name)
                    .xalign(0.0)
                    .ellipsize(gtk::pango::EllipsizeMode::End)
                    .build(),
            )
            .hexpand(true)
            .css_classes(["flat"])
            .build();
        let rename = Button::builder()
            .icon_name("document-edit-symbolic")
            .tooltip_text("Rename preset")
            .css_classes(["flat"])
            .build();
        let delete = Button::builder()
            .icon_name("user-trash-symbolic")
            .tooltip_text("Delete preset")
            .css_classes(["flat"])
            .build();

        // Rows are rebuilt on every refresh, dropping these closures.
        let (m, n) = (self.clone(), name.clone());
        load.connect_clicked(move |_| m.load(&n));
        let (m, n) = (self.clone(), name.clone());
        rename.connect_clicked(move |_| m.start_rename(&n));
        let m = self.clone();
        delete.connect_clicked(move |_| m.delete(&name));

        let row = GtkBox::builder().orientation(Orientation::Horizontal).build();
        row.append(&load);
        row.append(&rename);
        row.append(&delete);
        self.list.append(&row);
    }

    /// Inline editor: [name field][✓][✗], with an error line shown on failure.
    fn append_rename_row(self: &Rc<Self>, name: String) {
        let field = Entry::builder().text(&name).hexpand(true).build();
        let confirm = Button::builder()
            .icon_name("object-select-symbolic")
            .tooltip_text("Rename")
            .build();
        let cancel = Button::builder()
            .icon_name("window-close-symbolic")
            .tooltip_text("Cancel")
            .build();
        let error = Label::builder()
            .xalign(0.0)
            .wrap(true)
            .visible(false)
            .css_classes(["error", "caption"])
            .build();

        let m = self.clone();
        let (n, f, e) = (name.clone(), field.clone(), error.clone());
        confirm.connect_clicked(move |_| m.commit_rename(&n, &f, &e));
        let m = self.clone();
        let e = error.clone();
        field.connect_activate(move |f| m.commit_rename(&name, f, &e));
        let e = error.clone();
        field.connect_changed(move |f| {
            f.remove_css_class("error");
            e.set_visible(false);
        });
        let m = self.clone();
        cancel.connect_clicked(move |_| m.cancel_rename());

        let controls = GtkBox::builder()
            .orientation(Orientation::Horizontal)
            .css_classes(["linked"])
            .build();
        controls.append(&field);
        controls.append(&confirm);
        controls.append(&cancel);
        let row = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .spacing(4)
            .margin_top(2)
            .margin_bottom(2)
            .build();
        row.append(&controls);
        row.append(&error);
        self.list.append(&row);
        // GTK selects an entry's text when it takes focus, so typing replaces it.
        field.grab_focus();
    }
}
