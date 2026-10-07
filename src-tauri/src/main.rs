// Unter Windows im Release kein zusätzliches Konsolenfenster.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    stage_ingest_lib::run()
}
