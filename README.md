# Funliday CLI

> Experimental, unofficial command-line client for [Funliday](https://www.funliday.com/).
> Not affiliated with Funliday.

The goal is to let local AI agents such as Hermes manage a user's personal
Funliday itineraries through a small CLI and a safety-gated MCP server.

## Status

Browser-assisted login, personal-trip reads, trip creation/deletion, place
search, collection folders and saved places, custom places, fixed times, travel
durations, notes, and a safety-gated MCP server are implemented.

The older public developer portal exists at `api.funliday.com`, but currently
returns a Heroku application error. Funliday's separate public Data API exposes
aggregate travel datasets, not personal itinerary management.

## Install

macOS and Linux users can install a prebuilt binary without Rust:

```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://raw.githubusercontent.com/iml885203/funliday-cli/main/install.sh | sh
```

The installer places `funliday` in `~/.local/bin` by default. Windows users can
download `funliday-windows-x86_64.zip` from GitHub Releases and place
`funliday.exe` somewhere on `PATH`.

To build from source instead:

```bash
cargo install --git https://github.com/iml885203/funliday-cli
```

## Login and basic use

```bash
funliday login
funliday status
funliday trips list
funliday trips show <trip-id>
funliday cities search 福岡
funliday places search 櫛田神社
funliday collections folders
funliday collections list
funliday capabilities
```

Create and delete operations require an explicit confirmation flag. Creation
currently accepts the Funliday city ID returned by its autocomplete service:

```bash
funliday trips create --name "Fukuoka" --city 7868657 \
  --start 2026-10-20 --end 2026-10-21 --trip-type 1 --yes
funliday trips delete <container-id> --yes
funliday trips add-place <trip-id> --day 1 --poi <poi-id> --name 櫛田神社 \
  --latitude 33.59295 --longitude 130.41046 --stay-minutes 45 --yes
funliday trips add-custom-place <trip-id> --day 1 --name 飯店 \
  --address "Fukuoka, Japan" --latitude 33.59 --longitude 130.40 --yes
funliday trips set-place-time <trip-id> <item-id> \
  --start 11:00 --stay-minutes 60 --yes
funliday trips set-transport <trip-id> <item-id> \
  --duration-minutes 25 --yes
funliday trips use-auto-transport <trip-id> <item-id> --yes
funliday trips calculate-transport <trip-id> <item-id> --yes
funliday trips set-place-note <trip-id> <item-id> \
  --note "Remember to reserve" --yes
funliday trips show-place-note <trip-id> <item-id>
funliday trips delete-place <trip-id> <item-id> --yes
```

Save candidate places before deciding which day to visit them:

```bash
funliday collections create-folder --name "2026 九州候選景點" --yes
funliday collections add-place --poi <poi-id> --name 櫛田神社 \
  --latitude 33.59295 --longitude 130.41046 --folder <folder-id> --yes
funliday collections list --folder <folder-id>
funliday collections remove-place <collection-item-id> --folder <folder-id> --yes
funliday collections delete-folder <folder-id> --yes
```

Use `places search` first to obtain a POI ID and coordinates. The folder ID
comes from `collections folders`; removal uses the saved item's collection ID,
not its POI ID.

`trips list` returns both the itinerary `_id` and the `containerId`; deletion
uses the latter.

## MCP server

The stdio MCP server is read-only by default:

```bash
funliday mcp
```

It exposes trip and collection reads plus city/place search. To advertise write
tools, the host must start `funliday mcp --enable-write`; every write call must
additionally include `confirm: true`. Write tools cover trips, itineraries,
collection folders, and saving or removing candidate places.

Transportation fields belong to the departure place: an `<item-id>` describes
the segment from that place to the next place on the same day. Automatic route
calculation requires the POI Bank session captured by a recent `funliday login`.

### Hermes Agent

After installing and logging in, register the write-enabled MCP server:

```bash
hermes mcp add funliday --command "$(command -v funliday)" \
  --args mcp --enable-write
hermes mcp test funliday
```

Accept the prompt to enable all tools, then start a new Hermes session or
restart a running gateway. MCP write tools still require `confirm: true`, so
enabling them does not bypass the server's write guard.

Example verification prompt:

```text
Use the Funliday MCP tools to list my private trips. Do not modify anything.
```

## Authentication and privacy

Login opens Funliday's official page in an isolated Chrome profile. Passwords and
social-login credentials stay on that page. The resulting session token is stored
in the operating system's standard config directory as a mode-`0600` file on Unix;
the project does not require macOS Keychain.

Do not commit the local config directory, browser profile, tokens, captured
requests, or personal itinerary exports.

## Limitations

- This is an unofficial client for undocumented Funliday interfaces, which may
  change without notice.
- Native flight creation is available in the Funliday app but is not exposed by
  this CLI. Flights can be represented with custom travel time and notes.
- Login requires Chrome or a compatible Chromium installation. After login,
  normal CLI and MCP use is headless.
