# Nude-Moon validation

Native Rust port of the Keiyoushi source protocol at commit
`ee661f5233b346559fbdd53993d447c318c5aef0`, under Apache-2.0.
The pinned paths are `src/ru/nudemoon` and, for HenChan,
`lib-multisrc/multichan`. The original icon is resized to 128x128 PNG.
SDK revision: `ae6485e3c14ecd1fd5497d170eae884c68d04a87`.

## Public metadata check, 2026-11-04

The public catalog returned HTTP 403 (a site protection response), even with the normal NMfYa/nm_mobile preferences. Live catalog/search/reading are not verified from this development network.

No account credentials, titles or images were printed or saved. Only public
metadata responses were processed in memory. No reader/image URLs were fetched.
The source reports failed/protection responses explicitly; it does not bypass
login requirements, purchase content or fabricate an empty successful library.
The domain may be configured in source settings; web login follows the selected
domain. Saved cookies are scoped to the domain selected at login and are sent
only to page requests at that exact HTTPS origin, not through the image modifier.
Source ID is `ru.nudemoon`. Manga and chapter keys preserve the site's exact path,
which differs from Mihon source IDs and cannot be substituted into Mihon imports.

## Offline validation

11 synthetic WASM tests cover catalog/details/search encoding, exact keys,
chapter pagination accumulation/deduplication, reader image ordering and error
responses, and cookie sanitization. No test contacts the public site.
Formatting, JSON resources, SDK-pinned Cargo lockfile, `cargo metadata --locked
--offline` and PNG dimensions pass locally. Actual compilation/runtime tests run
on Linux CI; the Windows development machine has no MSVC linker.
