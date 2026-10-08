# ezone

ezone is a simple, vinyl-inspired music player. It brings the fun of playing physical records to your local digital music collection.

**Supported formats:** MP3, FLAC, WAV, OGG, M4A

## Controls

ezone relies mostly on simple keyboard shortcuts:

| Key         | Action                                      |
| ----------- | ------------------------------------------- |
| **`O`**     | open and scan a music folder                |
| **`Space`** | play/pause the current record               |
| **`L`**     | toggle library view                         |
| **`T`**     | toggle tracklist view                       |
| **`Esc`**   | return to Turntable view                    |
| **`Q`**     | quit the app                                |
| **`H`**     | toggle help overlay                         |

### Mouse interactions

* **Library view:** click any album cover to place that record on the turntable.
* **Turntable view:** click the grooves of the spinning record to jump to different tracks! The outer edge is the first track, and the inner edge near the label is the last track.
* **Tracklist view:** click a track name to jump right to it.

## Album art

ezone tries its best to find cover art to put on the record labels and in your library. It checks in this order:

1. Local image files in the album folder (`cover.jpg`, `folder.jpg`, `cover.png`).
2. Embedded metadata tags (ID3/Vorbis) inside the first audio file of the folder.

## Theming

When you open ezone for the first time, it creates a `theme.json` configuration file. The exact location depends on your operating system:

- **Linux:** `~/.config/ezone/theme.json`
- **Windows:** `%APPDATA%\ezone\theme.json`
- **macOS:** `~/Library/Application Support/ezone/theme.json`

You can open it with any text editor and change the RGB numbers to customize the background, text, accent, and record colors. ezone will automatically load your custom theme on the next startup.

## Folder structure

ezone treats every folder like a single vinyl record:

```text
my_cool_music/
├── Album Name - Artist Name/
│   ├── cover.jpg
│   ├── 01 - Track Title.flac
│   └── 02 - Another Track.flac
```

It reads titles, artists, and album names directly from the audio tags (ID3/FLAC). If your files don't have tags, ezone will try to guess them from the folder and file names. I wrote the code to handle messy file structures and weird naming conventions, but it might not always get it right.

## Installation

### Prebuilt binaries

You can just download the app for Linux and Windows from the [Releases](https://github.com/giovanni-norbedo/ezone/releases) page. MacOS is not officially supported, but you can try building it from source.

### Building from source

To build ezone from source, ensure you have Rust installed and run:

```bash
git clone https://github.com/giovanni-norbedo/ezone.git
cd ezone
cargo build --release
```

On Linux, ensure you have the required dependencies installed.

### AUR

If you are on an arch-based distribution, you can install ezone from the AUR:

```bash
yay -S ezone
# or
paru -S ezone
```

## Contributing

If you want to contribute to ezone, feel free to fork the repository and submit pull requests. I welcome any improvements, bug fixes, or new features!

## License

ezone is licensed under the GNU General Public License v3.0. See the [LICENSE](LICENSE) file for more details.