# Noise Generator
#
#   make                 build a release binary
#   make run             build and run it
#   make test            run the test suite
#   make install         install the binary, desktop entry and icons
#   make uninstall       remove everything `make install` put down
#   make clean           remove build output
#
# Installs into ~/.local by default, so no root is needed. For a system-wide
# install, override PREFIX:
#
#   make && sudo make install PREFIX=/usr/local
#
# DESTDIR is honored for staged installs (packaging), in which case the icon
# and desktop caches are left alone.

PREFIX  ?= $(HOME)/.local
BINDIR  ?= $(PREFIX)/bin
DATADIR ?= $(PREFIX)/share

CARGO   ?= cargo
INSTALL ?= install

BIN     := noise-generator
APP_ID  := us.jlong.NoiseGenerator
TARGET  := target/release/$(BIN)

DESKTOP       := data/$(APP_ID).desktop
ICON          := data/icons/hicolor/scalable/apps/$(APP_ID).svg
ICON_SYMBOLIC := data/icons/hicolor/symbolic/apps/$(APP_ID)-symbolic.svg

ICONDIR := $(DATADIR)/icons/hicolor

.PHONY: all build run test install uninstall clean

all: build

# Cargo does its own dependency tracking, so always hand off to it.
build:
	$(CARGO) build --release

run:
	$(CARGO) run --release

test:
	$(CARGO) test

# Doesn't build, so `sudo make install` never runs cargo as root.
install:
	@test -x $(TARGET) || { echo "$(TARGET) not found; run 'make' first" >&2; exit 1; }
	$(INSTALL) -Dm755 $(TARGET) $(DESTDIR)$(BINDIR)/$(BIN)
	$(INSTALL) -Dm644 $(DESKTOP) $(DESTDIR)$(DATADIR)/applications/$(APP_ID).desktop
	$(INSTALL) -Dm644 $(ICON) $(DESTDIR)$(ICONDIR)/scalable/apps/$(APP_ID).svg
	$(INSTALL) -Dm644 $(ICON_SYMBOLIC) $(DESTDIR)$(ICONDIR)/symbolic/apps/$(APP_ID)-symbolic.svg
	@$(MAKE) --no-print-directory refresh-caches

uninstall:
	rm -f $(DESTDIR)$(BINDIR)/$(BIN)
	rm -f $(DESTDIR)$(DATADIR)/applications/$(APP_ID).desktop
	rm -f $(DESTDIR)$(ICONDIR)/scalable/apps/$(APP_ID).svg
	rm -f $(DESTDIR)$(ICONDIR)/symbolic/apps/$(APP_ID)-symbolic.svg
	@$(MAKE) --no-print-directory refresh-caches

clean:
	$(CARGO) clean

# Tell the desktop about new or removed icons and launchers. Skipped for
# staged installs, and when the tools aren't present.
.PHONY: refresh-caches
refresh-caches:
ifeq ($(DESTDIR),)
	-@command -v gtk-update-icon-cache >/dev/null && \
		gtk-update-icon-cache -qtf $(ICONDIR) || true
	-@command -v update-desktop-database >/dev/null && \
		update-desktop-database -q $(DATADIR)/applications || true
endif
