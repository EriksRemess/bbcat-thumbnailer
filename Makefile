PREFIX ?= /usr/local
DESTDIR ?=
CARGO ?= $(HOME)/.cargo/bin/cargo
SUDO ?= sudo
THUMBNAILER_ENTRY = target/bbcat-thumbnailer.thumbnailer
LEGACY_USER_PREFIX = $(HOME)/.local

.PHONY: all build deb install uninstall $(THUMBNAILER_ENTRY)

all: build

build:
	"$(CARGO)" build --release --locked

deb:
	./scripts/build-deb

$(THUMBNAILER_ENTRY): data/bbcat-thumbnailer.thumbnailer.in Makefile
	mkdir -p target
	sed 's|@BINDIR@|$(PREFIX)/bin|g' $< > $@

install: build $(THUMBNAILER_ENTRY)
	$(SUDO) install -Dm755 target/release/bbcat-thumbnailer "$(DESTDIR)$(PREFIX)/bin/bbcat-thumbnailer"
	$(SUDO) install -Dm644 $(THUMBNAILER_ENTRY) "$(DESTDIR)$(PREFIX)/share/thumbnailers/bbcat-thumbnailer.thumbnailer"
	$(SUDO) install -Dm644 data/bbcat-thumbnailer.xml "$(DESTDIR)$(PREFIX)/share/mime/packages/bbcat-thumbnailer.xml"
	@if [ -z "$(DESTDIR)" ]; then \
		rm -f "$(LEGACY_USER_PREFIX)/bin/bbcat-thumbnailer" \
			"$(LEGACY_USER_PREFIX)/share/thumbnailers/bbcat-thumbnailer.thumbnailer" \
			"$(LEGACY_USER_PREFIX)/share/mime/packages/bbcat-thumbnailer.xml"; \
		if command -v update-mime-database >/dev/null; then \
			update-mime-database "$(LEGACY_USER_PREFIX)/share/mime"; \
		fi; \
	fi
	@if [ -z "$(DESTDIR)" ] && command -v update-mime-database >/dev/null; then \
		$(SUDO) update-mime-database "$(PREFIX)/share/mime"; \
	fi
uninstall:
	$(SUDO) rm -f "$(DESTDIR)$(PREFIX)/bin/bbcat-thumbnailer"
	$(SUDO) rm -f "$(DESTDIR)$(PREFIX)/share/thumbnailers/bbcat-thumbnailer.thumbnailer"
	$(SUDO) rm -f "$(DESTDIR)$(PREFIX)/share/mime/packages/bbcat-thumbnailer.xml"
	@if [ -z "$(DESTDIR)" ] && command -v update-mime-database >/dev/null; then \
		$(SUDO) update-mime-database "$(PREFIX)/share/mime"; \
	fi
