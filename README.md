# Noise Generator

A small GTK 4 desktop app, written in Rust, that plays continuous background
noise. It's shaped with a 10-band equalizer, and there are one-click presets
for the classic noise colors.

## Features

- **10-band octave equalizer** from 31 Hz to 16 kHz, ±30 dB per band.
- **Built-in presets** for the classic noise colors:

  | Preset | Slope         | Sounds like                     |
  | ------ | ------------- | ------------------------------- |
  | White  | flat          | TV static, bright hiss          |
  | Pink   | −3 dB/octave  | steady rain                     |
  | Brown  | −6 dB/octave  | waterfall, strong wind, rumble  |
  | Blue   | +3 dB/octave  | hissing spray                   |
  | Violet | +6 dB/octave  | very high-pitched hiss          |

- **Your own presets:** save the current EQ under a name, then load,
  rename or delete it from the **My Presets** menu.
- **Constant loudness:** the equalizer changes the tone, not the volume.
  Output is normalized, so switching presets doesn't jump in loudness.
- **Media keys and desktop integration** through
  [MPRIS](https://specifications.freedesktop.org/mpris-spec/latest/):
  Play/Pause works without the window focused, Next/Previous step through
  presets, and the app shows up in GNOME's and KDE's media controls with the
  current preset as the track title.
- **Remembers your settings:** the EQ and volume are restored the next time
  you start the app.
- **No clicks or pops:** volume, play/pause and EQ changes fade in over
  about 20 ms.

## Requirements

- Rust 1.92 or newer (required by the gtk4 bindings)
- GTK 4.12 or newer, with its development headers
- ALSA development headers, used by [`cpal`](https://crates.io/crates/cpal)
  for audio output. PipeWire and PulseAudio systems work through their ALSA
  compatibility layer.

Fedora:

```sh
sudo dnf install gtk4-devel alsa-lib-devel
```

Debian / Ubuntu:

```sh
sudo apt install libgtk-4-dev libasound2-dev
```

## Building and running

```sh
make        # build a release binary
make run    # build and run it
```

Launching the app while it's already running brings the existing window to
the front instead of opening a second one.

### Installing

To get the app in your launcher with its icon, install the binary, the
desktop entry and the icons into `~/.local`:

```sh
make
make install
```

`make uninstall` removes them again. For a system-wide install, set
`PREFIX`, and build first so cargo doesn't run as root:

```sh
make
sudo make install PREFIX=/usr/local
```

`DESTDIR` is supported for staged installs when packaging.

The icon shows the ten equalizer bands, colored brown, pink, white, blue and
violet from low to high frequency.

## Usage

1. Pick a noise color, or drag the equalizer sliders to shape your own sound.
2. Set **Volume** and press **Play**.
3. To keep a sound you like, open **My Presets**, type a name and press
   **Save** (or Enter). Saving under an existing name shows **Overwrite**.
   In the list, click a preset to load it, the pencil to rename it, or the
   trash can to delete it.

The band levels are relative to each other. Pulling every slider down equally
doesn't make the sound quieter; use **Volume** for that.

### Media keys

| Key / MPRIS action | Effect                                        |
| ------------------ | --------------------------------------------- |
| Play/Pause         | Toggle playback                               |
| Stop               | Pause                                         |
| Next / Previous    | Cycle built-in presets, then your saved ones  |

The app registers on the session bus as `org.mpris.MediaPlayer2.NoiseGenerator`,
so any MPRIS client works too, for example:

```sh
playerctl --player=NoiseGenerator play-pause
```

## Configuration files

Both files live in `$XDG_CONFIG_HOME/noise-generator/` (usually
`~/.config/noise-generator/`), are plain INI, and can be edited by hand
while the app is closed.

- **`settings.ini`**: the EQ and volume, saved when the window closes. Play
  state isn't saved, so the app always starts silent.
- **`presets.ini`**: your saved presets, one `[section]` per preset:

  ```ini
  [Rainy night]
  band_31=6
  band_63=3
  ...
  band_16k=-12
  ```

Missing or invalid values fall back to their defaults, and out-of-range
levels are clamped. If `presets.ini` can't be read at all, it's moved to
`presets.ini.bak` instead of being overwritten.

## How it works

Each octave band has its **own independent white-noise source**, passed
through a band-pass filter (an RBJ biquad with Q = √2, one octave wide) and
scaled by that band's slider. Because the band signals are uncorrelated,
their powers add without interfering with each other, so each slider's dB
value is exactly that band's level. With every slider at 0 dB the output is
white noise, and the colored presets are straight lines across the sliders.

Loudness normalization is calculated exactly rather than measured: the output
variance of the band-pass filter for white noise works out to α / (1 + α),
so the app can predict the output level of any EQ setting and scale it to a
fixed target.

The GTK thread and the real-time audio thread share state only through
atomics, so the audio callback never waits on a lock or allocates memory.

## Project layout

| File                   | Purpose                                                  |
| ---------------------- | -------------------------------------------------------- |
| `src/main.rs`          | Window layout and the My Presets popover                  |
| `src/player.rs`        | Shared playback state (play/pause, volume, preset)       |
| `src/audio.rs`         | `cpal` output stream and gain smoothing                  |
| `src/noise.rs`         | Band filters, noise generation, built-in presets         |
| `src/settings.rs`      | Loading and saving `settings.ini`                        |
| `src/user_presets.rs`  | Saving, loading, renaming and deleting presets           |
| `src/mpris.rs`         | MPRIS D-Bus interface                                     |
| `data/`                | Desktop entry and app icons (full-color and symbolic)    |

## Tests

```sh
make test
```

The tests check that every preset produces the target loudness at 44.1 and
48 kHz without clipping, and that settings and presets save and load
correctly, including invalid values, corrupt files and rename conflicts.

## License

Copyright (C) 2026 John Long

This program is free software: you can redistribute it and/or modify it under
the terms of the GNU General Public License as published by the Free Software
Foundation, either version 3 of the License, or (at your option) any later
version.

This program is distributed in the hope that it will be useful, but WITHOUT
ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
FOR A PARTICULAR PURPOSE. See the [GNU General Public License](LICENSE) for
more details.

The dependencies keep their own licenses: the Rust crates are MIT and/or
Apache-2.0, and the GTK and ALSA system libraries are LGPL-2.1-or-later. All
are compatible with GPL-3.0.
