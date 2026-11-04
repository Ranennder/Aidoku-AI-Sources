# Validation

Native code originates from Aidoku Community Sources commit `4ca350a031e6e708ad1308a6f4127a69fe517841`
(`ru.senkuro` is the base for `ru.senkognito`). SDK is pinned to
`ae6485e3c14ecd1fd5497d170eae884c68d04a87`; lockfile was regenerated
with Cargo 1.99.0. Source ID and existing manga/chapter key formats are retained.

## Public metadata smoke, 2026-11-04

Public browse metadata returned HTTP 200 (HTML). No account, chapter reader or page images were requested.

Requests used normal source headers, without credentials. Response bodies were
processed in memory; no catalog titles, account details or images were saved.
Account login, paid content and protected chapters are not validated by this check.
Access restrictions are preserved.

## Offline checks

5 WASM tests use synthetic keys, metadata or reader representations without
HTTP. Resource JSON and 128x128 PNG headers, exact SDK locks and formatting are
checked locally. Actual Rust/WASM compilation and runtime tests require Linux CI;
the Windows development machine has no MSVC linker.
