APP      := Mural.app
BINARY   := target/release/mural
CONTENTS := $(APP)/Contents
SIGN     := SnapLayout Dev
OFFLINE  ?= --offline

.PHONY: all build bundle run clean

all: bundle

build:
	cargo build $(OFFLINE) --release

bundle: build
	mkdir -p "$(CONTENTS)/MacOS" "$(CONTENTS)/Resources"
	cp Info.plist "$(CONTENTS)/Info.plist"
	cp Ressources/icones/mural.icns "$(CONTENTS)/Resources/mural.icns"
	cp LICENSE NOTICE.md "$(CONTENTS)/Resources/"
	cp "$(BINARY)" "$(CONTENTS)/MacOS/mural"
	codesign --force --sign "$(SIGN)" "$(APP)"
	@echo "Built $(APP)"

run: bundle
	open "$(APP)"

clean:
	cargo clean
	rm -rf "$(APP)"
