# Scenes and plugins

Open **Scenes** under **Plugins** in the desktop sidebar. Every MCP session connected to this build gets its own room, including sessions that have not joined the mesh. Restart existing harnesses after updating Synapse so their MCP processes can publish presence. Mesh lives under **Workspace**; Scenes does not require the mesh to be on.

Connections indicate configured harnesses; scene rooms indicate running MCP transports. Two sessions using the same harness get separate rooms. The server sends a heartbeat every ten seconds; an ungraceful exit becomes disconnected after 45 seconds. Graceful exits dim the room immediately. Rooms remain visible for up to a day and are reused on reconnect by harness, project folder, and room slot. Character choices survive reconnects.

Click a room to inspect its harness, project, last heartbeat, reported state, and task note. Previous and Next select its character. Filter by project or pause animation. A live connection with no work report displays **Connected**, not Working. Working, waiting, blocked, and finished poses follow mesh `reportstatus` calls. The mesh can remain off when only connection presence is needed. On macOS, Reduce Motion pauses animation by default unless you saved a different preference. Pause stops animation; presence continues updating every two seconds. Leaving Scenes stops its reads and animation.

## Add a scene plugin

Choose **Open plugins folder**. Create a lowercase directory for your pack and put a `plugin.toml` inside it. Choose **Reload plugins** to discover it. No rebuild or restart is required.

The default location is the Synapse application data directory's `plugins` folder. `SYNAPSE_DATA` relocates it along with the rest of Synapse's data.

The CLI provides a development loop that keeps a work-in-progress outside the data directory until it validates:

```sh
mkdir moonforge
synapse plugin init ./moonforge
synapse plugin check ./moonforge
synapse plugin install ./moonforge
synapse plugin list
```

`plugin init [folder]` adds a manifest to an existing art folder, derives its ID from the folder name, and accepts `--id <id>` when the folder name is not a valid plugin ID. It never overwrites `plugin.toml`. `plugin create <id> [folder]` is the convenience for making a brand-new folder and manifest together.

`plugin check` validates the manifest and every declared asset path without installing anything. `plugin install` repeats that validation and copies only `plugin.toml` and the assets it names into the local plugin folder. It refuses an existing ID unless you add `--replace`; a replacement keeps the previous installed directory as one bounded `.moonforge.synapsebackup` rollback copy. Run `synapse plugin path` to print the exact destination. If Scenes is already open, choose **Reload plugins** after an install.

```toml
api = 1
id = "moonforge"
name = "Moon forge"
version = "1.0.0"
kind = "scene"

[scene]
floor = "#87959c"
wall = "#464d68"
wood = "#61576c"
accent = "#b5d8df"
# Optional room art, exactly 144 by 104 pixels:
# background = "room.png"
# Optional character sheet, eight characters in a 4 by 2 grid:
# characters = "characters.png"
framewidth = 26
frameheight = 36
```

A character occupies three frame columns and four direction rows: down, left, right, up. The sheet therefore has twelve columns and eight rows of frames. Without a custom sheet, the renderer uses the bundled character set. A single custom sheet provides eight appearances repeated across the 32 character slots. Optional background artwork replaces the room's walls, floor, furniture, and props. The renderer overlays occupants and state effects; leave the central bench, fireplace at the upper right, and lower-left delivery area clear for these.

API 1 scene plugins are declarative packs: palettes, room art, and character sheets. Synapse owns room layout, session identity, state, animation timing, and selection. Packs cannot execute scripts or read memories, credentials, or files outside their own directory. This is the first plugin capability; it is not a general-purpose code extension runtime.

Plugin IDs contain 1 to 64 lowercase letters and digits, must be unique, and cannot replace the included `workshop`. Unknown fields, unsupported APIs or kinds, invalid colors, missing assets, path traversal, and symlinks outside a pack are rejected. Images must be PNG files up to 16 MiB and 4096 pixels per dimension; manifests are limited to 64 KiB. The CLI validates paths and file bounds; image decoding and dimensions are checked when the app loads the art. One invalid plugin does not prevent other scenes from loading. The selected scene and animation preference are saved locally.

## Artwork in source builds

The default room is original procedural pixel art. The optional Time Fantasy character sheets are read at build time from `assets/scene/characters/chara2.png` through `chara5.png`, copied from the owner's Tink purchase. The build embeds the available sheets, so the executable does not depend on the original asset directory. Keep paid source artwork out of public source distributions. Attribution is finalbossblues, Time Fantasy RPG Sprites 1: https://finalbossblues.itch.io/tf-rpg-charactersprites-1.

A checkout without the optional pack builds with a small original character sprite. No artwork is downloaded during build or rendering. Custom scene packs can provide their own appropriately licensed art.
