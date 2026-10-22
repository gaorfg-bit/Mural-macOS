APP      := Mural.app
BIN      := .build/release/Mural
CONTENTS := $(APP)/Contents
SIGN     := SnapLayout Dev

.PHONY: all build test render bundle run clean

all: bundle

build:
	swift build -c release

test:
	swift test

render:
	swift run -q Mural --rendu outils/rendu.png "$(DOSSIER)"

bundle: build
	mkdir -p "$(CONTENTS)/MacOS" "$(CONTENTS)/Resources"
	cp Info.plist "$(CONTENTS)/Info.plist"
	cp Ressources/icones/mural.icns "$(CONTENTS)/Resources/mural.icns" 2>/dev/null || cp Ressources/icones/mural.png "$(CONTENTS)/Resources/mural.icns"
	cp "$(BIN)" "$(CONTENTS)/MacOS/Mural"
	codesign --force --sign "$(SIGN)" "$(APP)"
	@echo "Built $(APP)"

run: bundle
	open "$(APP)"

clean:
	swift package clean
	rm -rf "$(APP)"
