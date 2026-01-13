# compare_icon_fonts
Compares paths for two Google-style icon fonts across a constellation of variation coordinates

```shell
$ rm -f /tmp/compare*
$ cargo run -- ~/Downloads/GoogleSymbols\[FILL,GRAD,ROND,opsz,wght]-2903-fontmake.ttf ~/Downloads/GoogleSymbols\[FILL,GRAD,ROND,opsz,wght]-2903-fontc-thomas.ttf
```

In case of failure two files are written to /tmp, e.g. `file:///tmp/compare.adb.fail.left.0.png` and `file:///tmp/compare.adb.fail.right.0.png`

In case of success one file is written to /tmp, e.g. `file:///tmp/compare.adb.pass.0.png`