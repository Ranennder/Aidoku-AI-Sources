# Senkognito

Native Aidoku source adapted from the pinned Aidoku Community Senkuro implementation.
The public Senkognito domain/API and app headers follow Keiyoushi multisrc Senkuro
configuration (commit `ee661f5233b346559fbdd53993d447c318c5aef0`).

Uses `https://senkognito.com` and `https://api.senkognito.com/graphql`. Login cookies
are isolated under source ID `ru.senkognito`; they are not shared with `ru.senkuro`.
The native chapter engine currently selects the primary translation branch.
Account-protected pages require the site's own web login; no access restrictions
are bypassed. Public availability and authenticated reading must be checked separately.
