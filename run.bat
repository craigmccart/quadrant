@echo off
:: Quadrant - build (debug) and run from source. Needs Rust (rustup.rs).
:: For a full clean, right-click and choose Run as administrator.
cd /d "%~dp0"
cargo run -p quadrant
