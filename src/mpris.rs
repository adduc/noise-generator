// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 John Long

//! MPRIS D-Bus interface (https://specifications.freedesktop.org/mpris-spec/latest/).
//! Desktops route hardware media keys (play/pause, next, previous) to MPRIS
//! players, so this makes them work without the window focused, and puts
//! the app in the shell's media controls. Next/Previous cycle presets.

use std::collections::HashMap;
use std::rc::Rc;

use gtk::ApplicationWindow;
use gtk::gio::{self, BusNameOwnerFlags, BusType, DBusConnection, DBusNodeInfo};
use gtk::glib::{self, Variant, variant::ObjectPath};
use gtk::prelude::*;

use crate::player::Player;

const BUS_NAME: &str = "org.mpris.MediaPlayer2.NoiseGenerator";
const OBJECT_PATH: &str = "/org/mpris/MediaPlayer2";
const ROOT_IFACE: &str = "org.mpris.MediaPlayer2";
const PLAYER_IFACE: &str = "org.mpris.MediaPlayer2.Player";
/// There's no real track, but clients expect a valid trackid in Metadata.
const TRACK_ID: &str = "/us/jlong/NoiseGenerator/CurrentSound";

const INTROSPECTION_XML: &str = r#"
<node>
  <interface name="org.mpris.MediaPlayer2">
    <method name="Raise"/>
    <method name="Quit"/>
    <property name="CanQuit" type="b" access="read"/>
    <property name="CanRaise" type="b" access="read"/>
    <property name="HasTrackList" type="b" access="read"/>
    <property name="Identity" type="s" access="read"/>
    <property name="SupportedUriSchemes" type="as" access="read"/>
    <property name="SupportedMimeTypes" type="as" access="read"/>
  </interface>
  <interface name="org.mpris.MediaPlayer2.Player">
    <method name="Next"/>
    <method name="Previous"/>
    <method name="Pause"/>
    <method name="PlayPause"/>
    <method name="Stop"/>
    <method name="Play"/>
    <method name="Seek">
      <arg direction="in" name="Offset" type="x"/>
    </method>
    <method name="SetPosition">
      <arg direction="in" name="TrackId" type="o"/>
      <arg direction="in" name="Position" type="x"/>
    </method>
    <method name="OpenUri">
      <arg direction="in" name="Uri" type="s"/>
    </method>
    <signal name="Seeked">
      <arg name="Position" type="x"/>
    </signal>
    <property name="PlaybackStatus" type="s" access="read"/>
    <property name="Rate" type="d" access="readwrite"/>
    <property name="Metadata" type="a{sv}" access="read"/>
    <property name="Volume" type="d" access="readwrite"/>
    <property name="Position" type="x" access="read"/>
    <property name="MinimumRate" type="d" access="read"/>
    <property name="MaximumRate" type="d" access="read"/>
    <property name="CanGoNext" type="b" access="read"/>
    <property name="CanGoPrevious" type="b" access="read"/>
    <property name="CanPlay" type="b" access="read"/>
    <property name="CanPause" type="b" access="read"/>
    <property name="CanSeek" type="b" access="read"/>
    <property name="CanControl" type="b" access="read"/>
  </interface>
</node>
"#;

/// Claims the MPRIS bus name and exports the player once the session bus
/// is available. Failure (e.g. no session bus) just means no media keys.
pub fn start(window: &ApplicationWindow, player: Rc<Player>) -> gio::OwnerId {
    let window = window.clone();
    gio::bus_own_name(
        BusType::Session,
        BUS_NAME,
        BusNameOwnerFlags::NONE,
        move |connection, _| {
            if let Err(err) = register(&connection, &window, &player) {
                eprintln!("failed to export MPRIS interface: {err}");
            }
        },
        |_, _| {},
        |_, name| eprintln!("could not own D-Bus name {name}; media keys won't work"),
    )
}

fn register(
    connection: &DBusConnection,
    window: &ApplicationWindow,
    player: &Rc<Player>,
) -> Result<(), glib::Error> {
    let node = DBusNodeInfo::for_xml(INTROSPECTION_XML)?;
    let root_info = node
        .lookup_interface(ROOT_IFACE)
        .expect("root interface in XML");
    let player_info = node
        .lookup_interface(PLAYER_IFACE)
        .expect("player interface in XML");

    let window = window.clone();
    connection
        .register_object(OBJECT_PATH, &root_info)
        .method_call(move |_, _, _, _, method, _, invocation| {
            match method {
                "Raise" => window.present(),
                // Closing (rather than quitting) runs the close handler, which saves settings.
                "Quit" => window.close(),
                _ => {}
            }
            invocation.return_value(None);
        })
        .property(|_, _, _, _, property| match property {
            "CanQuit" | "CanRaise" => true.to_variant(),
            "HasTrackList" => false.to_variant(),
            "Identity" => "Noise Generator".to_variant(),
            // SupportedUriSchemes, SupportedMimeTypes: we can't open anything.
            _ => Vec::<String>::new().to_variant(),
        })
        .build()?;

    let p = player.clone();
    let p2 = player.clone();
    connection
        .register_object(OBJECT_PATH, &player_info)
        .method_call(move |_, _, _, _, method, _, invocation| {
            match method {
                "Play" => p.set_playing(true),
                // There's no position to return to, so Stop is the same as Pause.
                "Pause" | "Stop" => p.set_playing(false),
                "PlayPause" => p.toggle(),
                "Next" => p.next_preset(),
                "Previous" => p.previous_preset(),
                _ => {} // Seek, SetPosition, OpenUri: no-ops, as CanSeek is false.
            }
            invocation.return_value(None);
        })
        .property(move |_, _, _, _, property| player_property(&p2, property))
        .set_property({
            let p = player.clone();
            move |_, _, _, _, property, value| {
                if property == "Volume"
                    && let Some(volume) = value.get::<f64>()
                {
                    p.set_volume(volume);
                }
                // Rate is fixed at 1.0; the spec says to ignore other values.
                true
            }
        })
        .build()?;

    // Tell clients (shell widgets, media key daemons) whenever state changes.
    let connection = connection.clone();
    let p = player.clone();
    player.connect_changed(move || {
        let changed: HashMap<String, Variant> = ["PlaybackStatus", "Metadata", "Volume"]
            .into_iter()
            .map(|name| (name.to_string(), player_property(&p, name)))
            .collect();
        let args = (PLAYER_IFACE, changed, Vec::<String>::new()).to_variant();
        if let Err(err) = connection.emit_signal(
            None,
            OBJECT_PATH,
            "org.freedesktop.DBus.Properties",
            "PropertiesChanged",
            Some(&args),
        ) {
            eprintln!("failed to emit MPRIS PropertiesChanged: {err}");
        }
    });
    Ok(())
}

fn player_property(player: &Player, property: &str) -> Variant {
    match property {
        "PlaybackStatus" => if player.is_playing() {
            "Playing"
        } else {
            "Paused"
        }
        .to_variant(),
        "Metadata" => metadata(player).to_variant(),
        "Volume" => player.volume().to_variant(),
        "Position" => 0i64.to_variant(),
        "Rate" | "MinimumRate" | "MaximumRate" => 1.0f64.to_variant(),
        "CanSeek" => false.to_variant(),
        // CanGoNext, CanGoPrevious, CanPlay, CanPause, CanControl
        _ => true.to_variant(),
    }
}

fn metadata(player: &Player) -> HashMap<String, Variant> {
    let track_id = ObjectPath::try_from(TRACK_ID.to_string()).expect("valid object path");
    HashMap::from([
        ("mpris:trackid".to_string(), track_id.to_variant()),
        ("xesam:title".to_string(), player.title().to_variant()),
        (
            "xesam:artist".to_string(),
            vec!["Noise Generator".to_string()].to_variant(),
        ),
    ])
}
