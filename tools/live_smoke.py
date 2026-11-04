#!/usr/bin/env python3
"""Read-only public-site checks, independent of the WASM parser test host.

Uses an in-memory cookie jar for MangaBuff's Laravel CSRF/session pairing.
No account, credentials, session values, chapter HTML, or images are persisted.
The only POST fetches the remaining public chapter-list HTML; image bodies are
sampled for 64 bytes. This checks site availability, not installed-app behavior.
"""

import argparse
import datetime
import http.cookiejar
import html.parser
import json
import pathlib
import time
import urllib.error
import urllib.parse
import urllib.request

DESKTOP_UA = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/132.0.0.0 Safari/537.36"
MOBILE_UA = "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1"
MAX_RESPONSE = 8 * 1024 * 1024


class Node:
    def __init__(self, tag="", attrs=()):
        self.tag, self.attrs, self.children = tag, dict(attrs), []

    def has_class(self, name):
        return name in self.attrs.get("class", "").split()

    def text(self):
        return " ".join(" ".join(c.text() if isinstance(c, Node) else c for c in self.children).split())

    def find(self, predicate):
        result = [self] if predicate(self) else []
        for child in self.children:
            if isinstance(child, Node):
                result.extend(child.find(predicate))
        return result


class Document(html.parser.HTMLParser):
    VOID = {"area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source", "track", "wbr"}

    def __init__(self, data):
        super().__init__(convert_charrefs=True)
        self.root = Node()
        self.stack = [self.root]
        self.feed(data.decode("utf-8", errors="replace"))

    def handle_starttag(self, tag, attrs):
        node = Node(tag, attrs)
        self.stack[-1].children.append(node)
        if tag not in self.VOID:
            self.stack.append(node)

    def handle_endtag(self, tag):
        for index in range(len(self.stack) - 1, 0, -1):
            if self.stack[index].tag == tag:
                self.stack = self.stack[:index]
                break

    def handle_data(self, data):
        self.stack[-1].children.append(data)


class Fetcher:
    def __init__(self, headers):
        self.headers = headers
        self.opener = urllib.request.build_opener(
            urllib.request.ProxyHandler({}),
            urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar()),
        )
        self.last_request = 0
        self.requests = []

    def fetch(self, url, *, headers=None, body=None, image=False):
        pause = 1 - (time.monotonic() - self.last_request)
        if pause > 0:
            time.sleep(pause)
        self.last_request = time.monotonic()
        request_headers = {**self.headers, **(headers or {})}
        assert not any(key.lower() in ("authorization", "cookie") for key in request_headers)
        request = urllib.request.Request(url, data=body, headers=request_headers)
        started = time.monotonic()
        try:
            response = self.opener.open(request, timeout=30)
        except urllib.error.HTTPError as error:
            response = error
        with response:
            data = response.read(64 if image else MAX_RESPONSE + 1)
            assert len(data) <= MAX_RESPONSE, "Unexpectedly large metadata response"
            status = response.status
            content_type = response.headers.get("Content-Type", "")
        self.requests.append({
            "method": "POST" if body is not None else "GET", "url": url,
            "status": status, "bytes_read": len(data), "content_type": content_type,
            "elapsed_seconds": round(time.monotonic() - started, 3),
        })
        assert status in ((200, 206) if image else (200,)), f"HTTP {status}: {url}"
        if image:
            assert content_type.lower().startswith("image/"), "Not an image response"
            valid_magic = (data.startswith((b"\xff\xd8\xff", b"\x89PNG\r\n\x1a\n", b"GIF87a", b"GIF89a"))
                           or (data[:4] == b"RIFF" and data[8:12] == b"WEBP")
                           or (data[4:8] == b"ftyp" and data[8:12] in (b"avif", b"avis")))
            assert valid_magic, "Unrecognized image bytes"
        return data

    def html(self, url, **kwargs):
        return Document(self.fetch(url, **kwargs)).root

    def json(self, url, **kwargs):
        return json.loads(self.fetch(url, **kwargs))


def manga_cards(document):
    items = []
    for card in document.find(lambda n: n.has_class("cards__item")):
        links = card.find(lambda n: n.tag == "a" and "/manga/" in n.attrs.get("href", ""))
        names = card.find(lambda n: n.has_class("cards__name"))
        if links and names:
            items.append((links[0].attrs["href"], names[0].text()))
    return items


def chapter_links(document):
    return [n.attrs["href"] for n in document.find(lambda n: n.tag == "a" and n.has_class("chapters__item") and "href" in n.attrs)]


def check_mangabuff():
    base = "https://mangabuff.ru"
    fetcher = Fetcher({"User-Agent": DESKTOP_UA, "Referer": base + "/"})
    result = {"cookie_jar_used": True, "requests": fetcher.requests}
    popular_doc = fetcher.html(base + "/manga?sort=real_views&page=1")
    popular = manga_cards(popular_doc)
    latest = manga_cards(fetcher.html(base + "/manga?sort=updated_at&page=1"))
    second = manga_cards(fetcher.html(base + "/manga?sort=real_views&page=2"))
    assert popular and latest and second and popular[0][0] != second[0][0]
    next_buttons = popular_doc.find(lambda n: n.has_class("pagination__button"))
    assert any(button.find(lambda n: n.tag == "a" and "href" in n.attrs) for button in next_buttons)
    query = urllib.parse.urlencode({"type": "manga", "q": "Герой", "page": 1})
    search = manga_cards(fetcher.html(base + "/search?" + query))
    assert search
    result.update(popular_count=len(popular), latest_count=len(latest), second_page_count=len(second), search_count=len(search))

    manga_url = base + "/manga/vozvrashchenie-gonchei-zheleznoi-krovi"
    details = fetcher.html(manga_url)
    assert details.find(lambda n: n.tag == "h1") and details.find(lambda n: n.has_class("manga__description"))
    initial = chapter_links(details)
    assert initial
    all_chapters = initial[:]
    if details.find(lambda n: n.has_class("load-chapters-trigger")):
        manga_id = details.find(lambda n: n.has_class("manga"))[0].attrs["data-id"]
        assert manga_id.isascii() and manga_id.isdigit()
        token_nodes = details.find(lambda n: n.tag == "meta" and n.attrs.get("name") == "csrf-token")
        assert token_nodes, "Missing CSRF metadata"
        token = token_nodes[0].attrs["content"]
        payload = fetcher.json(base + "/chapters/load", body=urllib.parse.urlencode({"manga_id": manga_id}).encode(), headers={
            "Origin": base, "X-Requested-With": "XMLHttpRequest", "X-CSRF-TOKEN": token,
            "Content-Type": "application/x-www-form-urlencoded; charset=UTF-8",
        })
        extra = chapter_links(Document(payload["content"].encode()).root)
        assert extra, "Remaining chapter list is empty"
        all_chapters.extend(extra)
        assert len(set(all_chapters)) > len(set(initial)), "No additional chapters were fetched"
    result.update(initial_chapters=len(initial), total_unique_chapters=len(set(all_chapters)))

    chapter_url = urllib.parse.urljoin(base, initial[-1])
    reader = fetcher.html(chapter_url)
    containers = reader.find(lambda n: n.has_class("reader__pages"))
    assert containers, "Public chapter has no reader container"
    images = containers[0].find(lambda n: n.tag == "img")
    urls = [urllib.parse.urljoin(base, n.attrs.get("data-src") or n.attrs.get("src", "")) for n in images]
    assert urls
    fetcher.fetch(urls[0], headers={"Referer": chapter_url, "Range": "bytes=0-63"}, image=True)
    result.update(reader_pages=len(urls), image_prefix_verified=True)
    return result


def check_mangalib():
    api = "https://api.cdnlibs.org"
    fetcher = Fetcher({"User-Agent": MOBILE_UA, "Site-Id": "1", "Origin": "https://mangalib.me", "Referer": api})
    result = {"cookie_jar_used": True, "requests": fetcher.requests}
    def query(path, parameters):
        return fetcher.json(api + path + "?" + urllib.parse.urlencode(parameters))
    popular = query("/api/manga", [("site_id[]", "1"), ("page", "1")])
    second = query("/api/manga", [("site_id[]", "1"), ("page", "2")])
    latest = query("/api/manga", [("site_id[]", "1"), ("sort_by", "last_chapter_at"), ("page", "1")])
    search = query("/api/manga", [("site_id[]", "1"), ("q", "Герой"), ("page", "1")])
    assert popular["data"] and second["data"] and latest["data"] and search["data"]
    assert popular["data"][0]["slug_url"] != second["data"][0]["slug_url"]
    slug = "29916--ag-yeog-uiending-eunjug-eumppun"
    details = query("/api/manga/" + slug, [("fields[]", value) for value in ("summary", "tags", "authors", "artists", "otherNames", "rate_avg")])["data"]
    assert details and details["slug_url"] == slug
    chapters = fetcher.json(api + "/api/manga/" + slug + "/chapters")["data"]
    selection = None
    for chapter in chapters:
        branches = chapter.get("branches", [])
        if isinstance(branches, dict):
            branches = branches.values()
        for branch in branches:
            restriction = branch.get("restricted_view") or {}
            moderation = branch.get("moderation") or {}
            if restriction.get("is_open", True) and moderation.get("label") != "На модерации":
                selection = (chapter, branch)
                break
        if selection:
            break
    assert selection, "No public open chapter branch"
    chapter, branch = selection
    parameters = [("number", chapter["number"]), ("volume", chapter["volume"])]
    if branch.get("branch_id") is not None:
        parameters.append(("branch_id", branch["branch_id"]))
    pages = query("/api/manga/" + slug + "/chapter", parameters)["data"]["pages"]
    assert pages
    if all(page.get("slug") is not None for page in pages):
        pages.sort(key=lambda page: int(page["slug"]))
    servers = query("/api/constants", [("fields[]", "imageServers")])["data"]["imageServers"]
    compatible_servers = [server for server in servers if "1" in [str(value) for value in server.get("site_ids", [])]]
    server = next((server for server in compatible_servers if server.get("id") == "compress"), compatible_servers[0])
    image_url = server["url"].rstrip("/") + "/" + pages[0]["url"].lstrip("/")
    fetcher.fetch(image_url, headers={"Range": "bytes=0-63"}, image=True)
    result.update(popular_count=len(popular["data"]), second_page_count=len(second["data"]), latest_count=len(latest["data"]), search_count=len(search["data"]), total_chapters=len(chapters), reader_pages=len(pages), image_prefix_verified=True)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", choices=("all", "mangabuff", "mangalib"), default="all")
    parser.add_argument("--output", type=pathlib.Path, default=pathlib.Path("build/live-smoke.json"))
    args = parser.parse_args()
    report = {
        "verified_at_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "scope": "Independent public-site HTTP smoke; not a WASM or installed-app test",
        "account_credentials_used": False, "proxy_used": False,
        "session_or_csrf_values_recorded": False, "chapter_html_or_images_persisted": False,
        "sources": {}, "errors": [],
    }
    for name, check in (("mangabuff", check_mangabuff), ("mangalib", check_mangalib)):
        if args.source not in ("all", name):
            continue
        try:
            report["sources"][name] = {"passed": True, **check()}
            print(name, "PASS", flush=True)
        except Exception as error:
            report["sources"][name] = {"passed": False, "error": str(error), "error_type": type(error).__name__}
            report["errors"].append(name + ": " + str(error))
            print(name, "FAIL", str(error), flush=True)
    report["all_passed"] = not report["errors"] and bool(report["sources"])
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"all_passed": report["all_passed"], "errors": report["errors"]}), flush=True)
    return 0 if report["all_passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
