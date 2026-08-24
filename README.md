# asciiquarium

A Rust terminal aquarium inspired by Kirk Baucom's original
[Asciiquarium](https://robobunny.com/projects/asciiquarium/html/). It preserves
the classic artwork and animation while requiring no Perl or CPAN runtime.

Kirk Baucom created the original program and animation engine. Most of the
ASCII artwork was drawn by Joan Stark, with later creatures contributed by
Claudio Matsuoka. Their work remains available here under GPL-2.0-or-later.

## Install

Download the `x86_64` or `aarch64` RPM from the
[latest GitHub release](https://github.com/Jmainguy/asciiquarium/releases/latest),
then:

```sh
sudo dnf install ./asciiquarium-*.rpm
```

## Usage

Run `asciiquarium`. Press `q`, Ctrl-C, Ctrl-D, or Escape to quit; `p` or Space
to pause; and `r` to rebuild the scene. Use `asciiquarium --classic` to limit
the aquarium to the original species.

## Build

```sh
cargo build --release --locked
```

Release tags build native RPMs on GitHub-hosted x86_64 and Arm64 runners. Each
release includes SHA-256 checksums alongside both packages.

## License

GPL-2.0-or-later. Copyright for the original program and artwork remains with
Kirk Baucom and its other credited contributors. Rust implementation changes
are distributed under the same license.
