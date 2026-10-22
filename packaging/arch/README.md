# Arch binary package

The checked-in PKGBUILD reproduces the published 2.0.1 standalone package. For each new release, `packaging/offline/arch.py` sets the current version and SHA256 from the just-built portable tarball and publishes the corresponding updated PKGBUILD with the new binary package. This avoids pointing at a nonexistent release archive while CI is building the release.

The offline bundle includes the resulting binary package and the complete official dependency closure. The plain `.pkg.tar.zst` uses pacman dependencies normally; use `offline.bin` when no dependency downloads should be needed.
