default: run

run:
    cargo run

test:
    cargo test

[working-directory: 'resources']
icon:
    magick Icon.png -define icon:auto-resize="64,48,32,16" Icon.ico

pkg:
    cargo packager --release
