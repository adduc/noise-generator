mod audio;
mod noise;

use std::cell::Cell;
use std::sync::Arc;

use gtk::prelude::*;
use gtk::{glib, Align, Application, ApplicationWindow, Box as GtkBox, Button, Label, Orientation, Scale, ToggleButton};

use audio::Controls;
use noise::NoiseColor;

const APP_ID: &str = "us.jlong.NoiseGenerator";

fn main() -> glib::ExitCode {
    let controls = Controls::new();

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
    app.connect_activate(move |app| build_ui(app, controls.clone()));
    app.run()
}

fn build_ui(app: &Application, controls: Arc<Controls>) {
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

    // Color selector: a row of linked toggle buttons acting as radio buttons.
    let color_row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .halign(Align::Center)
        .css_classes(["linked"])
        .build();

    let description = Label::builder()
        .label(NoiseColor::White.description())
        .wrap(true)
        .justify(gtk::Justification::Center)
        .width_chars(40)
        .max_width_chars(40)
        .css_classes(["dim-label"])
        .build();

    let mut first: Option<ToggleButton> = None;
    for color in NoiseColor::ALL {
        let button = ToggleButton::with_label(color.label());
        if let Some(first) = &first {
            button.set_group(Some(first));
        } else {
            button.set_active(true);
            first = Some(button.clone());
        }
        let controls = controls.clone();
        let description = description.clone();
        button.connect_toggled(move |b| {
            if b.is_active() {
                controls.set_color(color);
                description.set_label(color.description());
            }
        });
        color_row.append(&button);
    }
    root.append(&color_row);
    root.append(&description);

    // Volume
    let volume_row = GtkBox::builder().orientation(Orientation::Horizontal).spacing(12).build();
    volume_row.append(&Label::new(Some("Volume")));
    let volume = Scale::with_range(Orientation::Horizontal, 0.0, 1.0, 0.01);
    volume.set_value(0.5);
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
    window.present();
}
