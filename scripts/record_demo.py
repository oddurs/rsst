#!/usr/bin/env python3
"""Capture a short rsst session as an asciinema v2 cast.

Drives the real binary in a pty and records what it writes, with timings. The
keystrokes below are a tour, not a script the app is aware of.
"""
import fcntl, json, os, pty, select, struct, subprocess, sys, termios, time

COLS, ROWS = 104, 30

# (keys, how long to linger afterwards) — paced for a human to follow.
TOUR = [
    ("", 1.6),           # land, feeds fetched
    ("\t", 1.0),         # focus the entries
    ("j", 0.9),
    ("j", 1.2),
    ("\t", 1.3),         # into the detail pane
    ("jjj", 1.2),        # scroll the article
    ("\t", 0.8),
    ("s", 1.0),          # star it
    ("u", 1.3),          # unread only
    ("u", 0.8),
    ("/", 0.8),          # search
    ("rust", 1.6),
    ("\x1b", 1.0),       # escape out
    ("?", 2.2),          # the key reference
    ("\x1b", 0.8),
    ("q", 0.4),
]

def main() -> int:
    binary = "./target/release/rsst"
    if not os.path.exists(binary):
        binary = "./target/debug/rsst"
    argv = [binary]
    if len(sys.argv) > 1:
        argv += ["--config", sys.argv[1]]

    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", ROWS, COLS, 0, 0))
    env = dict(os.environ, TERM="xterm-256color")
    proc = subprocess.Popen(argv, stdin=slave, stdout=slave, stderr=slave,
                            close_fds=True, env=env)
    os.close(slave)

    start = time.time()
    events = []

    def pump(seconds):
        end = time.time() + seconds
        while time.time() < end:
            ready, _, _ = select.select([master], [], [], 0.02)
            if not ready:
                continue
            try:
                chunk = os.read(master, 65536)
            except OSError:
                return
            if chunk:
                events.append([round(time.time() - start, 4), "o",
                               chunk.decode("utf-8", "replace")])

    for keys, linger in TOUR:
        for key in keys:
            os.write(master, key.encode())
            pump(0.12)
        pump(linger)

    try:
        proc.wait(timeout=3)
    except subprocess.TimeoutExpired:
        proc.kill()
    os.close(master)

    header = {"version": 2, "width": COLS, "height": ROWS,
              "timestamp": int(start), "title": "rsst",
              "env": {"TERM": "xterm-256color", "SHELL": "/bin/sh"}}
    out = [json.dumps(header)] + [json.dumps(e) for e in events]
    print("\n".join(out))

    duration = events[-1][0] if events else 0
    print(f"recorded {len(events)} events over {duration:.1f}s", file=sys.stderr)
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
