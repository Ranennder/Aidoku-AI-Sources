# MangaLib validation

Based on `sources/ru.mangalib` and `templates/libgroup` from
[Aidoku Community Sources](https://github.com/Aidoku-Community/sources/tree/4ca350a031e6e708ad1308a6f4127a69fe517841),
commit `4ca350a031e6e708ad1308a6f4127a69fe517841`. The upstream MIT/Apache-2.0 notices
are retained at the repository root. Aidoku AI version 10 adds query escaping,
numeric page ordering, exact chapter-branch validation with refreshed chapter
caching, and the existing account
notification handler export. Manga and chapter identities remain compatible with
this upstream Aidoku source.

Version 11 rejects failed HTTP responses before decoding manga records, keeps a
one-hour chapter cache even when migration initializes it first, and resolves
the image CDN once per chapter. A temporary constants failure can use a previously
validated CDN; without one it reports an error instead of returning relative image
URLs. Model-based text attachments also propagate missing-CDN errors. No source,
manga, chapter, account or settings keys were changed.

## Public HTTP check, 2026-11-04

Checked from the development PC using Python `urllib`, without account cookies,
access tokens or changes to a site account. Requests used the source's normal
`Site-Id: 1`, `Origin`, `Referer` and Safari user-agent headers.

| Check | Result |
| --- | --- |
| `/api/manga?site_id[]=1&page=1` | HTTP 200, 60 catalog entries |
| `/api/constants?fields[]=imageServers` | HTTP 200, CDN configuration returned |
| `/api/manga/29916--ag-yeog-uiending-eunjug-eumppun` with requested detail fields | HTTP 200, structured summary |
| Same manga `/chapters` | HTTP 200, 215 chapters |
| Same manga `/chapter?number=0&volume=1`, open branch | HTTP 200, 7 image pages, numeric slugs 1–7 |
| First image on the returned compression CDN, HEAD only | HTTP 200, `image/jpeg`, 749241 bytes |
| Unknown manga detail URL | HTTP 404, JSON `data.toast` error envelope |
| `/api/auth/me`, without an account | HTTP 401, JSON `data.toast` error envelope |
| Daily trending, repeated after the v11 changes | HTTP 200, 56 entries and pagination metadata |

The final v11 public check also repeated image-server constants (HTTP 200, 14
configured servers) and both error envelopes above. The runtime error regressions
use synthetic responses with the same shape; no live response bodies were saved.

No manga text or page images were saved. A request without the normal source
headers returned HTTP 403; those headers must be retained. Website access can
still depend on region, server availability and anti-bot policy.

These HTTP checks verify the public API path, not rendering on an iPhone. Default
WASM tests use fixed data without HTTP: special-character searches, repeated
filters, exact and missing branch IDs, refreshed branch caches, numeric page ordering, legacy page arrays,
and restricted-chapter flags. CI builds and runs them separately.
Version 11 adds offline regressions for error envelopes and private diagnostic
payloads, migration-first cache expiry, cold CDN failures, recovery and stale-CDN
fallback, and whole-chapter image URL generation. These tests do not fetch pages.

Account login uses the source's existing SocialLib web login and local-storage
token support. Restricted chapters remain locked; this source does not unlock or
purchase them. Authenticated access was not tested with a personal account.

The native source ID is `ru.mangalib`; manga keys are `slug_url` strings and chapter
keys are numeric branch record IDs. Mihon uses different source IDs and chapter
URL keys. Existing Suwayomi-based Mihon imports continue using their explicit
server mapping; copying those keys directly into this native source is unsupported.
