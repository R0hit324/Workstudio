# Nexus Code

A collaborative IDE for the **local network** — a TUI (terminal) editor where your
team can write, run, and share code in real time from any device on the same LAN
(any subnet), with no cloud account and no internet required.

Built in Rust on [ratatui](https://ratatui.rs) + [tui-textarea](https://crates.io/crates/tui-textarea).

```
Host device:   nexus                        # binds 0.0.0.0:8245
Other devices: nexus --connect 192.168.1.10:8245
```

## How it works

- **Host mode (default):** the device running `nexus` opens a WebSocket server on
  `0.0.0.0:<port>` (default `8245`), so any device on the network — across all
  subnets — can reach it via the host's IP. The host keeps the authoritative copy
  of every file.
- **Workspace directory:** on setup (or `--dir <path>`) the host picks a real
  directory. Its contents are scanned on start and synced to every user. When you
  point it at an existing project folder, that project becomes the live workspace.
  No directory chosen? It falls back to the room-based data dir
  (`~/.local/share/nexus/workspace/<room>/`).
- **Join mode:** `nexus --connect <host-ip>:<port>` connects to a host over
  WebSocket. Every edit is sent as a line-diff patch; the host relays patches,
  cursors, and presence to all other connected devices.
- **Web browser:** any device can open `http://<host-ip>:<port>/` in a browser
  to get a live, collaborative editor served by the host on the same port. The
  browser joins the same WebSocket session: it shows the workspace files and
  who's online in real time, and its edits are folded into the shared store —
  so a web edit shows up in the TUI, gets persisted, and lands in the git
  commit just like any other user's. TUI edits are pushed to every open browser
  as whole-file updates automatically.
- **Git:** if the host's directory is a git repository, pressing `Ctrl+S` stages
  and commits the workspace as the **person who saved** (`git log` attribution via
  `--author`) — the session owner when the host saves, or the joiner's display
  name when a joiner saves. Non-git directories just save to disk. A branch +
  commit-count indicator appears in the status bar, and `Ctrl+J` opens the
  commit log.
- **Joining late:** a new device receives a full snapshot (all files + current
  members) the moment it connects, then stays in sync via patches.
- **Conflict resolution (rebase + last-writer-wins):** the host serializes all
  patches. A patch that applies cleanly to the current revision is applied and
  echoed to the room. A patch whose base is behind gets **rebased** onto the
  current content when its removed lines are still present, and the transformed
  patch is relayed; otherwise (or if the sender is ahead) the patch is dropped
  and that peer is re-synced with a snapshot — so a simultaneous same-line edit
  keeps exactly one winner everywhere. Distinct-region edits from different
  users merge cleanly; overlapping edits converge to the last writer, and every
  screen plus the on-disk file stay identical.

## Getting started

```sh
# build
cargo build --release

# host a workspace (any device)
./target/release/nexus

# join from another device on the network
./target/release/nexus --connect 192.168.1.10:8245
```

On first launch you'll be asked for a display name and whether to **Host** a new
session or **Join** an existing device (plus the host address). On later launches
it remembers your last session. Press `Ctrl+G` in the editor to open the invite
panel with the exact join command to share with your team.

## CLI options

| Flag                | Description                                                        |
|---------------------|--------------------------------------------------------------------|
| `--name <NAME>`     | Display name                                                       |
| `--room <ROOM>`     | Room name (shared session label)                                   |
| `--dir <PATH>`      | Host workspace directory (host mode); git repo → commits on save   |
| `--port <PORT>`     | Host port (default `8245`); host binds `0.0.0.0` so all subnets work |
| `--connect <HOST:PORT>` | Join an existing host instead of hosting                       |
| `--webcompat`       | Legacy whole-file `code`/`presence` broadcasts (browsers now work automatically via the host's translation) |

## Keybindings

| Keys               | Action                                   |
|--------------------|------------------------------------------|
| `Ctrl+S`           | Save (host writes workspace files; commits to git if a repo) |
| `Ctrl+J`           | Git log (host, when workspace is a git repo)                  |
| `Ctrl+N`           | New file (language menu)                                      |
| `Ctrl+N`           | New file (language menu)                 |
| `Ctrl+F`           | Format code                              |
| `Ctrl+T`           | Toggle dark/light theme                  |
| `Ctrl+G`           | Invite panel (join command)              |
| `Ctrl+Q`           | Quit                                     |
| `F5`               | Run current file                         |
| `F6`               | Toggle theme                             |
| `F1` / `?`         | Help                                     |
| `F4`               | Close current file                       |
| `Tab`              | Autocomplete / navigate (setup screen)   |
| `Esc`              | Close modal / dismiss autocomplete       |

### Running code

- **Python** runs locally on each device (`python3 -u`) — works offline, no
  install beyond Python itself.
- **Other languages** run through the [Piston](https://github.com/engineer-man/piston)
  API. Note: the public Piston endpoint is whitelist-only as of 2/15/2026 — host
  your own Piston instance (or set the URL in `src/runner.rs`) if you need
  non-Python execution.

## Roadmap

- [x] LAN host/join sync (all subnets) with presence, live cursors, and persistence
- [x] Git: host commits workspace dir as session owner; status bar + git log modal
- [x] Multiple cursors UX polish for multiple users
- [x] Git support for all users (joiners, not just the host)
- [x] Web-app interop via the same LAN WebSocket
