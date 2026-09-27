# Funliday CLI

> Experimental, unofficial command-line client for [Funliday](https://www.funliday.com/).
> Not affiliated with Funliday.

The goal is to let local AI agents such as Hermes manage a user's personal
Funliday itineraries through a small CLI and a safety-gated MCP server.

## Status

This repository is in active API-discovery stage. Browser-assisted login and
read-only access to personal trips are implemented. Write commands and MCP tools
will be added after their request formats have been safely validated.

The older public developer portal exists at `api.funliday.com`, but currently
returns a Heroku application error. Funliday's separate public Data API exposes
aggregate travel datasets, not personal itinerary management.

## Build and try

```bash
cargo build --release
./target/release/funliday login
./target/release/funliday status
./target/release/funliday trips list
./target/release/funliday trips show <trip-id>
./target/release/funliday capabilities
```

Login opens Funliday's official page in an isolated Chrome profile. Passwords and
social-login credentials stay on that page. The resulting session token is stored
in the operating system's standard config directory as a mode-`0600` file on Unix;
the project does not require macOS Keychain.

Do not commit the local config directory, browser profile, tokens, captured
requests, or personal itinerary exports.
