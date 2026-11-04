# MangaLib validation

Based on `sources/ru.mangalib` and `templates/libgroup` from
[Aidoku Community Sources](https://github.com/Aidoku-Community/sources/tree/4ca350a031e6e708ad1308a6f4127a69fe517841),
commit `4ca350a031e6e708ad1308a6f4127a69fe517841`. The upstream MIT/Apache-2.0 notices
are retained at the repository root. Aidoku AI version 10 adds query escaping,
numeric page ordering, exact chapter-branch validation with refreshed chapter
caching, and the existing account
notification handler export. Manga and chapter identities remain compatible with
this upstream Aidoku source.

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

No manga text or page images were saved. A request without the normal source
headers returned HTTP 403; those headers must be retained. Website access can
still depend on region, server availability and anti-bot policy.

These HTTP checks verify the public API path, not rendering on an iPhone. Default
WASM tests use fixed data without HTTP: special-character searches, repeated
filters, exact and missing branch IDs, refreshed branch caches, numeric page ordering, legacy page arrays,
and restricted-chapter flags. CI builds and runs them separately.

Account login uses the source's existing SocialLib web login and local-storage
token support. Restricted chapters remain locked; this source does not unlock or
purchase them. Authenticated access was not tested with a personal account.

The native source ID is `ru.mangalib`; manga keys are `slug_url` strings and chapter
keys are numeric branch record IDs. Mihon uses different source IDs and chapter
URL keys. Existing Suwayomi-based Mihon imports continue using their explicit
server mapping; copying those keys directly into this native source is unsupported.
