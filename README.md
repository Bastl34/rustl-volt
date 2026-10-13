# Rustl-Volt

A remake of Re-Volt on the [rustl](https://github.com/Bastl34/rustl) engine.

No files of the original game are in this repository. Rustl-Volt converts them from your own copy the first time it starts.

## Screenshots

![Menu](screenshots/screenshot_menu.webp)

![Race](screenshots/screenshot_game.webp)

![Race](screenshots/screenshot_game2.webp)

![Race](screenshots/screenshot_game3.webp)

## What you need

- **Re-Volt** from Steam ([store page](https://store.steampowered.com/app/287310/ReVolt/)), installed. The game finds it on its own. If it does not, it asks you for the folder.
- **[rustl](https://github.com/Bastl34/rustl)** checked out, plus its toolchain (Rust and Node.js, see its README).

## Setup

1. Clone this repository into the rustl folder, as `data/projects/rustl-volt`. `code/Cargo.toml` and `converter/` find the engine at `../../../..`.
2. Start the rustl editor (`cargo dev`) and open `rustl-volt.project`.
3. Press Play in the `boot` scene. At the first start the game converts Re-Volt into `assets/` and the scenes, then it opens the menu.

## Folders

- `code/` - the game (app code of the project)
- `converter/` - the converter, as a library for the game and as a window for development (`cargo run` there)
- `ui/` - own pictures of the start screens
- `formats/` - notes on the file formats of the original game


## Detailed infos about Re-Volt

Detailed info about all files and formats can be found here: [Readme.md](formats/README.md)

Also there are some schematics here: [Readme.md](schematics/README.md)

## Legal

Rustl-Volt is a non-commercial fan project. It is not affiliated with, endorsed by or connected to the owners of Re-Volt.

Re-Volt and all of its content (models, textures, tracks, sounds, music, names) belong to their respective owners. Re-Volt is a trademark of its owners.

This repository contains no files of the original game. Rustl-Volt only converts the files of a copy you own, on your own machine. To play, you need your own copy of Re-Volt, for example [on Steam](https://store.steampowered.com/app/287310/ReVolt/).

The screenshots show the game converted from such a copy. They are only here to show the project. If you own rights to Re-Volt and want anything removed, please open an issue and it will be taken down.

The code of Rustl-Volt, the converter and the notes on the file formats are written from scratch. The original program was only studied to understand how the game behaves; none of its code is copied.