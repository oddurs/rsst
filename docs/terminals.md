# Terminals

What has actually been tried, and what has only been reasoned about. The second
list is longer than the first, and saying so is the point of this page.

## Verified

| Terminal | Platform | Result |
| -------- | -------- | ------ |
| Apple Terminal | macOS | Box drawing, 16 colours, truecolor via `#rrggbb`. Key handling correct. |
| pseudo-terminal (`pty`, `TERM=xterm-256color`) | macOS | Used by the test harness. Enters and leaves the alternate screen cleanly, restores on panic. |
| `TestBackend` | all | Every rendering test. Not a real terminal, but it is what pins the layout at 80×24. |

## Reasoned about, not verified

These rest on crossterm behaving as documented rather than on someone having
run rsst in them:

| Terminal | Platform | Expectation |
| -------- | -------- | ----------- |
| iTerm2, Alacritty, kitty, WezTerm | macOS / Linux | Box drawing and truecolor. No known issues. |
| GNOME Terminal, Konsole, xterm | Linux | As above; xterm may be limited to 256 colours. |
| Windows Terminal | Windows | Expected to work. Key handling is covered by a test, see below. |
| conhost (`cmd.exe`) | Windows | Box drawing depends on the console font. Set `ascii = true` if borders come out as blocks. |
| tmux / screen | all | Pass through; colour depends on the multiplexer's own `TERM`. |
| Linux virtual console | Linux | Box drawing usually works; colour is 16 at most. |

## The Windows double-keystroke

Windows reports a `Release` for every key as well as a `Press`, and some
terminals add `Repeat`. Acting on all of them makes every keystroke happen two
or three times, which looks like a rendering bug rather than an input one.

`handles()` in `main.rs` accepts only `Press`, and there is a test asserting
that `Release` and `Repeat` are both ignored. The test runs on Windows in CI
along with everything else, so the guarantee does not depend on anyone
remembering this page.

## ASCII fallback

Not every terminal and font can draw `─│┌┐└┘`, and a non-UTF-8 locale turns them
into mojibake — which is worse than plain ASCII, because it looks broken rather
than plain.

```toml
[theme]
ascii = true
```

Unset means detect: ASCII is used when `TERM` is `dumb` or unset, or when the
locale is set and is not UTF-8. Setting it explicitly overrides the detection
either way.

In ASCII mode **every** character rsst draws is ASCII — borders, the selection
cursor, the star, the fold arrows, the status separator, the loading marker, and
the placeholder for an undated entry. A test renders a screen with all of those
present and fails on any character above 127, so this cannot rot by someone
adding one more glyph. A second test asserts the default theme still uses box
drawing, so the fallback cannot quietly become the only mode.

## The mouse

rsst asks for SGR mouse reporting (`1000`/`1002`/`1006`), which every terminal in
the lists above supports. Capture is released on exit — including on a panic,
because a terminal left in mouse-reporting mode prints escape codes at the shell
prompt on every click, which looks like a broken shell rather than a broken
reader.

Holding **Shift** while dragging gives selection back to the terminal in
Ghostty, iTerm2, kitty and Alacritty. `mouse = false` turns capture off entirely.

## Reporting one

If rsst looks wrong in your terminal, `rsst --version`, the terminal's name, and
`echo $TERM` are what make it actionable.
