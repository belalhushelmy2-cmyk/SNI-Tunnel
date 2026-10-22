# Offline release builds

`arch.py --assets DIR --output DIR` builds the Arch `-bin` package from the current portable tarball, resolves dependencies against an empty local pacman database using only core/extra, verifies each dependency's repository SHA256 and package signature, creates a local repository, and writes a dependency manifest. It never reads profiles or changes the build host's installed packages.

`ubuntu.sh [ASSET_DIR]` runs on Ubuntu 24.04. Apt resolves/downloads the full closure with an empty status file, even for libraries installed on the build machine. Apt's normal repository authentication remains enabled. `dpkg-scanpackages` creates the offline index.

`finish.py DIR PLATFORM OUTPUT` writes checksums and creates both an archive and a self-extracting `.bin`. `--check` verifies the outer payload without installation; `--extract DIRECTORY` unpacks it without installation. `install.sh --check` verifies all inner files.

The installer uses the target machine's package manager to resolve required packages, and never blindly extracts system files over an installation. The Arch installer refuses to replace existing dependency versions (a full system update is required instead). No passwords, polkit bypass rules, user profiles or global proxy configuration are embedded.

Supported release platforms: Arch/Omarchy x86_64 and Ubuntu 24.04 x86_64. A working base OS, package manager, sudo and desktop session are prerequisites. The Windows ZIP contains the same runtime/driver files as the NSIS installer; portable TAP setup still requires Administrator permission.
