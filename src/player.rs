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
use crate::noise::{BAND_COUNT, Preset};
use crate::user_presets::PresetLibrary;

#[derive(Clone, Debug, PartialEq)]
pub enum PresetRef {
    Builtin(Preset),
    User(String),
}

/// The preset the sliders currently match, or none once hand-tweaked.
/// Kept free of widgets so its rules can be unit-tested; `Player` applies
/// the results and notifies listeners. Setters return whether it changed.
#[derive(Debug, Default)]
struct CurrentPreset(Option<PresetRef>);

impl CurrentPreset {
    fn get(&self) -> Option<&PresetRef> {
        self.0.as_ref()
    }

    fn set(&mut self, preset: Option<PresetRef>) -> bool {
        if self.0 == preset {
            return false;
        }
        self.0 = preset;
        true
    }

    /// Accepts a preset remembered from the last run only if it still exists
    /// (`saved` is its levels, if so) and the sliders still match it: it may
    /// have been edited or deleted while the app was closed.
    fn restore(
        &mut self,
        preset: PresetRef,
        saved: Option<[f32; BAND_COUNT]>,
        sliders: [f32; BAND_COUNT],
    ) -> bool {
        saved == Some(sliders) && self.set(Some(preset))
    }

    fn is_user_preset(&self, name: &str) -> bool {
        matches!(&self.0, Some(PresetRef::User(n)) if n == name)
    }

    fn renamed(&mut self, old: &str, new: &str) -> bool {
        self.is_user_preset(old) && self.set(Some(PresetRef::User(new.to_string())))
    }

    fn deleted(&mut self, name: &str) -> bool {
        self.is_user_preset(name) && self.set(None)
    }

    /// Display name, e.g. "Pink noise" or a preset name.
    fn title(&self) -> String {
        match &self.0 {
            Some(PresetRef::Builtin(preset)) => format!("{} noise", preset.label()),
            Some(PresetRef::User(name)) => name.clone(),
            None => "Custom EQ".to_string(),
        }
    }

    /// The preset `delta` places away in `cycle`, wrapping around.
    /// `cycle` must not be empty.
    fn step<'a>(&self, cycle: &'a [PresetRef], delta: isize) -> &'a PresetRef {
        let len = cycle.len() as isize;
        let position = cycle.iter().position(|p| Some(p) == self.get());
        let index = match position {
            Some(i) => (i as isize + delta).rem_euclid(len),
            // From a custom EQ, "next" starts at the top and "previous" at the end.
            None if delta > 0 => 0,
            None => len - 1,
        };
        &cycle[index as usize]
    }
}

pub struct Player {
    controls: Arc<Controls>,
    library: Rc<PresetLibrary>,
    sliders: Vec<Scale>,
    volume: Scale,
    play_button: Button,
    playing: Cell<bool>,
    current: RefCell<CurrentPreset>,
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
            current: RefCell::default(),
            applying: Cell::new(false),
            listeners: RefCell::new(Vec::new()),
        });

        // Handlers hold strong refs: the player lives as long as the window.
        for (band, slider) in player.sliders.iter().enumerate() {
            let p = player.clone();
            slider.connect_value_changed(move |s| {
                p.controls.set_band_db(band, s.value() as f32);
                if !p.applying.get() {
                    p.update_current(|c| c.set(None));
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

    /// Changes the current preset, notifying listeners if it changed. The
    /// borrow ends before notifying, since listeners read the current preset.
    fn update_current(&self, change: impl FnOnce(&mut CurrentPreset) -> bool) {
        let changed = change(&mut self.current.borrow_mut());
        if changed {
            self.notify();
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
        self.play_button
            .set_label(if playing { "Pause" } else { "Play" });
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
        self.current.borrow().title()
    }

    pub fn current(&self) -> Option<PresetRef> {
        self.current.borrow().get().cloned()
    }

    /// A preset's levels, or None if it's a user preset that no longer exists.
    fn preset_band_db(&self, preset: &PresetRef) -> Option<[f32; BAND_COUNT]> {
        match preset {
            PresetRef::Builtin(p) => Some(p.band_db()),
            PresetRef::User(name) => self.library.get(name),
        }
    }

    /// Sets the sliders to a preset. Returns false if a user preset is gone.
    pub fn apply(&self, preset: PresetRef) -> bool {
        let Some(band_db) = self.preset_band_db(&preset) else {
            return false;
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
        self.update_current(|c| c.set(preset));
    }

    /// Marks a preset remembered from the last run as current, if it still
    /// exists and matches the sliders.
    pub fn restore_current(&self, preset: PresetRef) {
        let saved = self.preset_band_db(&preset);
        let sliders = self.band_db();
        self.update_current(|c| c.restore(preset, saved, sliders));
    }

    pub fn preset_renamed(&self, old: &str, new: &str) {
        self.update_current(|c| c.renamed(old, new));
    }

    pub fn preset_deleted(&self, name: &str) {
        self.update_current(|c| c.deleted(name));
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
        let next = self.current.borrow().step(&cycle, delta).clone();
        self.apply(next);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(name: &str) -> PresetRef {
        PresetRef::User(name.to_string())
    }

    fn current(preset: Option<PresetRef>) -> CurrentPreset {
        CurrentPreset(preset)
    }

    #[test]
    fn set_reports_only_real_changes() {
        let mut c = current(None);
        assert!(!c.set(None));
        assert!(c.set(Some(user("Rain"))));
        assert!(!c.set(Some(user("Rain"))));
        assert!(c.set(None)); // a slider tweak
        assert_eq!(c.get(), None);
    }

    #[test]
    fn titles() {
        assert_eq!(current(None).title(), "Custom EQ");
        assert_eq!(
            current(Some(PresetRef::Builtin(Preset::Pink))).title(),
            "Pink noise"
        );
        assert_eq!(current(Some(user("Rain"))).title(), "Rain");
    }

    #[test]
    fn renaming_the_current_preset_follows_it() {
        let mut c = current(Some(user("rain")));
        assert!(c.renamed("rain", "Rain"));
        assert_eq!(c.get(), Some(&user("Rain")));
    }

    #[test]
    fn renaming_another_preset_changes_nothing() {
        let mut c = current(Some(user("Rain")));
        assert!(!c.renamed("Wind", "Gale"));
        assert_eq!(c.get(), Some(&user("Rain")));

        // A built-in sharing the old name is not a user preset.
        let mut c = current(Some(PresetRef::Builtin(Preset::Pink)));
        assert!(!c.renamed("Pink", "Rose"));
        assert_eq!(c.get(), Some(&PresetRef::Builtin(Preset::Pink)));
    }

    #[test]
    fn deleting_the_current_preset_clears_it() {
        let mut c = current(Some(user("Rain")));
        assert!(c.deleted("Rain"));
        assert_eq!(c.get(), None);
        assert_eq!(c.title(), "Custom EQ");
    }

    #[test]
    fn deleting_another_preset_changes_nothing() {
        let mut c = current(Some(user("Rain")));
        assert!(!c.deleted("Wind"));
        assert_eq!(c.get(), Some(&user("Rain")));

        let mut c = current(None);
        assert!(!c.deleted("Rain"));
    }

    #[test]
    fn restore_requires_an_unchanged_preset() {
        let levels = Preset::Pink.band_db();
        let rain = user("Rain");

        let mut c = current(None);
        assert!(!c.restore(rain.clone(), None, levels)); // deleted since
        assert!(!c.restore(rain.clone(), Some([0.0; BAND_COUNT]), levels)); // edited since
        assert_eq!(c.get(), None);

        assert!(c.restore(rain.clone(), Some(levels), levels));
        assert_eq!(c.get(), Some(&rain));
    }

    fn cycle() -> Vec<PresetRef> {
        Preset::ALL
            .into_iter()
            .map(PresetRef::Builtin)
            .chain([user("Rain"), user("Wind")])
            .collect()
    }

    #[test]
    fn step_moves_and_wraps_both_ways() {
        let cycle = cycle();
        let first = &cycle[0];
        let last = cycle.last().unwrap();

        assert_eq!(current(Some(first.clone())).step(&cycle, 1), &cycle[1]);
        assert_eq!(current(Some(cycle[1].clone())).step(&cycle, -1), first);
        assert_eq!(current(Some(last.clone())).step(&cycle, 1), first);
        assert_eq!(current(Some(first.clone())).step(&cycle, -1), last);
    }

    #[test]
    fn step_from_custom_eq_starts_at_an_end() {
        let cycle = cycle();
        assert_eq!(current(None).step(&cycle, 1), &cycle[0]);
        assert_eq!(current(None).step(&cycle, -1), cycle.last().unwrap());
    }

    #[test]
    fn step_from_a_preset_missing_from_the_cycle_starts_at_an_end() {
        // Treated like a custom EQ rather than panicking.
        let cycle = cycle();
        assert_eq!(current(Some(user("Gone"))).step(&cycle, 1), &cycle[0]);
    }
}
