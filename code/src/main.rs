// the game without editor (exports) - release builds open no console window on windows
#![cfg_attr(all(windows, not(debug_assertions), not(feature = "dynamic")), windows_subsystem = "windows")]

fn main()
{
    rustl::run_app(rustl_volt::rustl_create_app);
}
