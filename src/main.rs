mod audio;
mod noise;
mod settings;

use std::cell::Cell;
use std::sync::Arc;

use gtk::prelude::*;
use gtk::{
    glib, Align, Application, ApplicationWindow, Box as GtkBox, Button, Label, Orientation,
    PositionType, Scale,
};

use audio::Controls;
use noise::{Preset, BAND_COUNT, MAX_DB, MIN_DB};
use settings::Settings;

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

    // Presets: set the sliders to a classic noise color, which you can then tweak.
    let preset_row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .halign(Align::Center)
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
        preset_row.append(&button);
    }
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
