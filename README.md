# compare_icon_fonts
Compares paths for two Google-style icon fonts across a constellation of variation coordinates

```shell
$ rm -f /tmp/compare*
$ cargo run -- ~/Downloads/GoogleSymbols\[FILL,GRAD,ROND,opsz,wght]-2903-fontmake.ttf ~/Downloads/GoogleSymbols\[FILL,GRAD,ROND,opsz,wght]-2903-fontc-thomas.ttf
```

In case of failure two files are written to /tmp, e.g. `/tmp/failure.desktop_portrait.left.32.svg` and `/tmp/failure.desktop_portrait.right.32.svg`

**CAVEAT** point order differences currently cause spurious diffs. See https://github.com/googlefonts/fontc/blob/0267a52700707e70efe98d762484e867eee7a8a8/ttx_diff/src/ttx_diff/core.py#L511 for context.