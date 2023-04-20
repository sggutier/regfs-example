Example taken from [prjfs-rs](https://github.com/fanzeyi/prjfs-rs/tree/master/examples/regfs), plus a couple of minor changes

# How to use

This can be run with `$env:RUST_LOG="placeholder=info"; cargo run`. This will mount a FS equivalent of the Windows Registry on `.\test`.

Creating a string value named `bruh` under any directory (e.g., `Computer\HKEY_CURRENT_USER\Control Panel`) on `regedit.exe` will create a directory simlink on `.\test\Computer\HKEY_CURRENT_USER\Control Panel\bruh` pointing to a directory named `Keyboard`, next to it.