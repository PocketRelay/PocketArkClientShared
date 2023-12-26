# Pocket Ark Client Shared

![License](https://img.shields.io/github/license/PocketRelay/PocketArkClientShared?style=for-the-badge)
![Build](https://img.shields.io/github/actions/workflow/status/PocketRelay/PocketArkClientShared/build.yml?style=for-the-badge)
![Cargo Version](https://img.shields.io/crates/v/pocket-ark-client-shared?style=for-the-badge)
![Cargo Downloads](https://img.shields.io/crates/d/pocket-ark-client-shared?style=for-the-badge)

[Discord Server (discord.gg/yvycWW8RgR)](https://discord.gg/yvycWW8RgR)
[Website (pocket-relay.pages.dev)](https://pocket-relay.pages.dev/)

This is a shared backend implementation for the Pocket Ark client variants so that they can share behavior without creating duplicated code and to make changes more easy to carry across between implementations

```toml
[dependencies]
pocket-ark-client-shared = "0.1"
```

## Used by

This shared backend is used by the following Pocket Ark projects:
- Standalone Client - https://github.com/PocketRelay/PocketArkClient
  - This is a standalone executable for the client
- ASI Plugin - https://github.com/PocketRelay/PocketArkClientPlugin
  - This is a plugin variant of the client loaded by Ansel64 plugin loaders



