// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 John Long

//! UI-side playback state: play/pause, volume, and which preset is active.
//! The window, the presets popover and MPRIS all go through `Player`, so
//! they always agree, and listeners hear about every change.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use gtk::prelude::*;
use gtk::{Button, Scale};

use crate::audio::Controls;
use crate::noise::{Preset, BAND_COUNT};
use crate::user_presets::PresetLibrary;

#[derive(Clone, Debug, PartialEq)]
pub enum PresetRef {
    Builtin(Preset),
    User(String),
}

pub struct Player {
    controls: Arc<Controls>,
    library: Rc<PresetLibrary>,
    sliders: Vec<Scale>,
    volume: Scale,
    play_button: Button,
    playing: Cell<bool>,
    /// The preset the sliders currently match, or None once hand-tweaked.
    current: RefCell<Option<PresetRef>>,
    /// True while a preset is moving the sliders, so that doesn't count as a tweak.
    applying: Cell<bool>,
    listeners: RefCell<Vec<Box<dyn Fn()>>>,
}

impl Player {
    /// Takes over the given widgets' signals: sliders and volume feed the
    /// audio controls, and the button toggles playback.
    pub fn new(
        controls: Arc<Controls>,
        library: Rc<PresetLibrary>,
        sliders: Vec<Scale>,
        volume: Scale,
        play_button: Button,
    ) -> Rc<Self> {
        let player = Rc::new(Self {
            controls,
            library,
            sliders,
            volume,
            play_button,
            playing: Cell::new(false),
            current: RefCell::new(None),
            applying: Cell::new(false),
            listeners: RefCell::new(Vec::new()),
        });

        // Handlers hold strong refs: the player lives as long as the window.
        for (band, slider) in player.sliders.iter().enumerate() {
            let p = player.clone();
            slider.connect_value_changed(move |s| {
                p.controls.set_band_db(band, s.value() as f32);
                if !p.applying.get() && p.current.replace(None).is_some() {
                    p.notify();
                }
            });
        }
        let p = player.clone();
        player.volume.connect_value_changed(move |s| {
            p.controls.set_volume(s.value() as f32);
            p.notify();
        });
        let p = player.clone();
        player.play_button.connect_clicked(move |_| p.toggle());
        player
    }

    pub fn library(&self) -> &PresetLibrary {
        &self.library
    }

    /// Registers a callback run after any change to playback, volume or preset.
    pub fn connect_changed(&self, f: impl Fn() + 'static) {
        self.listeners.borrow_mut().push(Box::new(f));
    }

    fn notify(&self) {
        for listener in self.listeners.borrow().iter() {
            listener();
        }
    }

    pub fn is_playing(&self) -> bool {
        self.playing.get()
    }

    pub fn set_playing(&self, playing: bool) {
        if self.playing.replace(playing) == playing {
            return;
        }
        self.controls.set_playing(playing);
        self.play_button.set_label(if playing { "Pause" } else { "Play" });
        self.notify();
    }

    pub fn toggle(&self) {
        self.set_playing(!self.is_playing());
    }

    pub fn volume(&self) -> f64 {
        self.volume.value()
    }

    /// Moves the volume slider, whose handler updates audio and listeners.
    pub fn set_volume(&self, volume: f64) {
        self.volume.set_value(volume.clamp(0.0, 1.0));
    }

    pub fn band_db(&self) -> [f32; BAND_COUNT] {
        std::array::from_fn(|band| self.sliders[band].value() as f32)
    }

    /// Display name for the current sound, e.g. "Pink noise" or a preset name.
    pub fn title(&self) -> String {
        match &*self.current.borrow() {
            Some(PresetRef::Builtin(preset)) => format!("{} noise", preset.label()),
            Some(PresetRef::User(name)) => name.clone(),
            None => "Custom EQ".to_string(),
        }
    }

    /// Sets the sliders to a preset. Returns false if a user preset is gone.
    pub fn apply(&self, preset: PresetRef) -> bool {
        let band_db = match &preset {
            PresetRef::Builtin(p) => p.band_db(),
            PresetRef::User(name) => match self.library.get(name) {
                Some(band_db) => band_db,
                None => return false,
            },
        };
        self.applying.set(true);
        for (slider, db) in self.sliders.iter().zip(band_db) {
            slider.set_value(db as f64);
        }
        self.applying.set(false);
        self.mark_current(Some(preset));
        true
    }

    /// Records which preset the sliders match without moving them (e.g.
    /// right after saving the current EQ under a name).
    pub fn mark_current(&self, preset: Option<PresetRef>) {
        if *self.current.borrow() != preset {
            self.current.replace(preset);
            self.notify();
        }
    }

    pub fn preset_renamed(&self, old: &str, new: &str) {
        if *self.current.borrow() == Some(PresetRef::User(old.to_string())) {
            self.mark_current(Some(PresetRef::User(new.to_string())));
        }
    }

    pub fn preset_deleted(&self, name: &str) {
        if *self.current.borrow() == Some(PresetRef::User(name.to_string())) {
            self.mark_current(None);
        }
    }

    /// Steps through built-in presets, then saved ones, wrapping around.
    pub fn next_preset(&self) {
        self.step_preset(1);
    }

    pub fn previous_preset(&self) {
        self.step_preset(-1);
    }

    fn step_preset(&self, delta: isize) {
        let cycle: Vec<PresetRef> = Preset::ALL
            .into_iter()
            .map(PresetRef::Builtin)
            .chain(self.library.names().into_iter().map(PresetRef::User))
            .collect();
        let len = cycle.len() as isize;
        let position = cycle.iter().position(|p| Some(p) == self.current.borrow().as_ref());
        let index = match position {
            Some(i) => (i as isize + delta).rem_euclid(len),
            // From a custom EQ, "next" starts at the top and "previous" at the end.
            None if delta > 0 => 0,
            None => len - 1,
        };
        self.apply(cycle[index as usize].clone());
    }
}
