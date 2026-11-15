# liwin task runner. `just --list` shows all recipes.

uuid := "liwin@kura"
ext_build := "extension/build" / uuid
user_ext_dir := env("XDG_DATA_HOME", env("HOME") / ".local/share") / "gnome-shell/extensions"

default:
    @just --list

# Build the Rust workspace and the extension.
build: ext-build
    cargo build --workspace

# Run Rust unit + D-Bus integration tests and type-check the extension.
test: ext-deps
    cargo test --workspace
    cd extension && npx tsc --noEmit

# Lint everything.
lint: ext-deps
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    cd extension && npx tsc --noEmit

ext-deps:
    cd extension && [ -d node_modules ] || npm ci

# Compile the extension (TypeScript → GJS ESM) into extension/build/liwin@kura.
ext-build: ext-deps
    cd extension && npx tsc
    rm -rf {{ext_build}}
    mkdir -p {{ext_build}}
    cp extension/metadata.json extension/dist/*.js {{ext_build}}/

# Pack the extension as a zip (for `gnome-extensions install`).
ext-pack: ext-build
    cd {{ext_build}} && gnome-extensions pack --force --out-dir=../ .

# Install into the REAL user extension dir. On Wayland the running Shell only
# picks it up after logging out and in; then `gnome-extensions enable liwin@kura`.
ext-install: ext-build
    mkdir -p {{user_ext_dir}}
    rm -rf {{user_ext_dir}}/{{uuid}}
    cp -r {{ext_build}} {{user_ext_dir}}/

ext-uninstall:
    rm -rf {{user_ext_dir}}/{{uuid}}

# Nested GNOME Shell in a window (needs mutter-dev-bin), isolated in .sandbox/.
nested: build
    scripts/nested.sh devkit

# Headless nested GNOME Shell, isolated in .sandbox/.
headless: build
    scripts/nested.sh headless

clean:
    cargo clean
    rm -rf extension/dist extension/build .sandbox
