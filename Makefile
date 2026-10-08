PREFIX   ?= /usr/local
DESTDIR  ?=
UNITDIR  ?= /etc/systemd/system
UDEVDIR  ?= /etc/udev/rules.d

BIN := target/release/arctis-rs

.PHONY: build install uninstall

# Forced build (always runs cargo).
build:
	cargo build --release --locked

# No prerequisites on purpose: if the binary exists, cargo is not run (so
# "sudo make install" does not rebuild as root). Run "make build" to refresh it.
$(BIN):
	cargo build --release --locked

install: $(BIN)
	install -Dm755 $(BIN) $(DESTDIR)$(PREFIX)/bin/arctis-rs
	install -Dm644 udev/70-arctis.rules $(DESTDIR)$(UDEVDIR)/70-arctis.rules
	sed 's|/usr/local/bin|$(PREFIX)/bin|' systemd/arctis-rs.service > arctis-rs.service.tmp
	install -Dm644 arctis-rs.service.tmp $(DESTDIR)$(UNITDIR)/arctis-rs.service
	rm -f arctis-rs.service.tmp

uninstall:
	rm -f $(DESTDIR)$(PREFIX)/bin/arctis-rs \
	      $(DESTDIR)$(UDEVDIR)/70-arctis.rules \
	      $(DESTDIR)$(UNITDIR)/arctis-rs.service
