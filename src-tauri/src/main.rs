// Oculta la consola adicional en Windows para la build de release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    parcut_lib::run();
}
