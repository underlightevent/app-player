// Empêche l'ouverture d'une console noire sous Windows en version finale.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    dj_library_manager_lib::run()
}
