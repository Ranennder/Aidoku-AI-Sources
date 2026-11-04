# Validation

Native code originates from Aidoku Community Sources commit `4ca350a031e6e708ad1308a6f4127a69fe517841`
(`ru.senkuro` is the base for `ru.senkognito`). SDK is pinned to
`ae6485e3c14ecd1fd5497d170eae884c68d04a87`; lockfile was regenerated
with Cargo 1.99.0. Source ID and existing manga/chapter key formats are retained.

## Public metadata smoke, 2026-11-04

The API returned HTTP 403 from the development network. This is a recorded access limitation; authenticated reading and regional access were not verified.

Requests used normal source headers, without credentials. Response bodies were
processed in memory; no catalog titles, account details or images were saved.
Account login, paid content and protected chapters are not validated by this check.
Access restrictions are preserved.

## Offline checks

6 WASM tests use synthetic keys, metadata or reader representations without
HTTP. Resource JSON and 128x128 PNG headers, exact SDK locks and formatting are
checked locally. Actual Rust/WASM compilation and runtime tests require Linux CI;
the Windows development machine has no MSVC linker.

The inherited native engine selects the primary translation branch (or the first
branch when no primary branch is marked). Alternative branches are not exposed.

Web login uses Aidoku AI 0.9.9 default cookie saving, including parent-domain SSO
cookies. No unrelated chapter is requested to verify an account, and no fixed
User-Agent overrides the real WebView/network identity. The source reads
the `.keys`/`.values` cookie map on every API request; logout clears it.
An additional offline regression covers that native storage path without HTTP.
