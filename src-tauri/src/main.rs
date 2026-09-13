// A status pet has no console to show, and on Windows one would appear behind
// it every launch.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    clawd_lib::run()
}
