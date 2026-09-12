#!/usr/bin/env python3
"""Serve the feed fixtures, and the failures a reader has to survive.

Static files come from `fixtures/feeds`. Everything else is generated, because
a 5,000-entry feed is better computed than committed, and a 404 is not a file.

Conditional requests are answered properly — ETag, Last-Modified, 304 — so the
caching path is exercised rather than bypassed. The content is a pure function
of the path, so every run serves byte-identical bytes.
"""

import hashlib
import http.server
import socketserver
import sys
import threading
import time
from email.utils import formatdate
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent / "fixtures" / "feeds"

# A fixed instant, so Last-Modified does not change from run to run.
EPOCH = 1789000000  # 2026-09-08T...Z


def huge(count: int) -> bytes:
    """A feed with more entries than anyone will scroll through by hand."""
    head = (
        '<?xml version="1.0" encoding="utf-8"?>\n'
        '<feed xmlns="http://www.w3.org/2005/Atom">\n'
        "  <title>Ten Thousand Things</title>\n"
        "  <id>urn:rsst:fixture:huge</id>\n"
        f"  <updated>2026-09-10T00:00:00Z</updated>\n"
    )
    body = []
    for n in range(count):
        # Dates march backwards a day at a time from a fixed start.
        day = 28 - (n % 28)
        month = 9 - (n // 28) % 9 or 1
        body.append(
            f"  <entry><id>urn:rsst:fixture:huge:{n}</id>"
            f"<title>Entry number {n}, of which there are {count}</title>"
            f'<link href="http://example.invalid/huge/{n}"/>'
            f"<updated>2026-{month:02d}-{day:02d}T00:00:00Z</updated>"
            f"<summary>Body text for entry {n}. It exists so the list has "
            f"something to show and the detail pane something to render.</summary>"
            "</entry>\n"
        )
    return (head + "".join(body) + "</feed>\n").encode()


def generated(path: str):
    """(status, content type, body) for a generated path, or None."""
    if path == "/huge.xml":
        return 200, "application/atom+xml", huge(5000)
    if path == "/medium.xml":
        return 200, "application/atom+xml", huge(120)
    if path == "/gone.xml":
        return 404, "text/plain", b"no such feed\n"
    if path == "/broken.xml":
        return 500, "text/plain", b"the server is having a day\n"
    if path == "/not-a-feed.xml":
        # Served with a feed's content type, which is how this one hurts.
        return 200, "application/atom+xml", b"<html><body>Not a feed.</body></html>\n"
    if path == "/truncated.xml":
        return 200, "application/atom+xml", (
            b'<?xml version="1.0"?>\n<feed xmlns="http://www.w3.org/2005/Atom">\n'
            b"  <title>Cut off mid-"
        )
    return None


class Handler(http.server.BaseHTTPRequestHandler):
    # Quiet by default: the point is the reader's output, not a request log.
    def log_message(self, *_args):
        if VERBOSE:
            sys.stderr.write("fixtures: %s\n" % (_args[0] % _args[1:]))

    def resolve(self):
        path = self.path.split("?", 1)[0]
        if path in ("/", "/index.html"):
            listing = "\n".join(sorted(p.name for p in ROOT.glob("*.xml")))
            return 200, "text/plain", (listing + "\n").encode()

        made = generated(path)
        if made is not None:
            return made

        # Static, and strictly inside the fixture directory.
        name = Path(path.lstrip("/")).name
        file = ROOT / name
        if not file.is_file():
            return 404, "text/plain", b"no such fixture\n"
        return 200, "application/atom+xml", file.read_bytes()

    def do_GET(self, body=True):
        if self.path.startswith("/slow"):
            # Slower than any sensible timeout, to see the pane say so.
            time.sleep(30)

        status, kind, payload = self.resolve()
        etag = '"%s"' % hashlib.sha256(payload).hexdigest()[:16]
        modified = formatdate(EPOCH, usegmt=True)

        if status == 200 and (
            self.headers.get("If-None-Match") == etag
            or self.headers.get("If-Modified-Since") == modified
        ):
            self.send_response(304)
            self.send_header("ETag", etag)
            self.send_header("Last-Modified", modified)
            self.end_headers()
            return

        self.send_response(status)
        self.send_header("Content-Type", kind)
        self.send_header("Content-Length", str(len(payload)))
        if status == 200:
            self.send_header("ETag", etag)
            self.send_header("Last-Modified", modified)
        self.end_headers()
        if body:
            self.wfile.write(payload)

    def do_HEAD(self):
        self.do_GET(body=False)


class Server(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
    daemon_threads = True


VERBOSE = False

if __name__ == "__main__":
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8787
    VERBOSE = "--verbose" in sys.argv
    with Server(("127.0.0.1", port), Handler) as server:
        threading.Thread(target=server.serve_forever, daemon=True).start()
        print(f"fixtures: serving {ROOT} on http://127.0.0.1:{port}", file=sys.stderr)
        try:
            while True:
                time.sleep(3600)
        except KeyboardInterrupt:
            pass
