set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]

# By default run test
default:
    just --list

# Run project tests
test:
    cargo test

# Run various checks in CI
ci: test clippy-ci fmt-ci

# Run clippy in error mode
clippy-ci:
    cargo clippy -- -Dwarnings

# Run fmt in check mode
fmt-ci:
    cargo fmt --check

# Generate icons from Icon.png
[working-directory: 'resources']
icon:
    magick Icon.png -define icon:auto-resize="64,48,32,16" Icon.ico

# Create packages
pkg:
    cargo packager --release

# Create Zip archive (windows only)
[windows]
zip:
    build/mk-archive.ps1