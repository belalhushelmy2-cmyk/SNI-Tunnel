#!/usr/bin/env bash
# Run as the desktop user. Package managers elevate only the install transaction.
set -euo pipefail
cd -- "$(dirname -- "$0")"
[[ $(uname -m) == x86_64 ]] || { echo 'This bundle requires x86_64.'; exit 1; }
sha256sum --quiet -c SHA256SUMS
if [[ ${1:-} == --check ]]; then echo 'Bundle checksums verified.'; exit 0; fi
[[ $EUID != 0 ]] || { echo 'Run ./install.sh as your normal desktop user, without sudo.'; exit 1; }
source /etc/os-release
bundle=$(cat platform)
tmp=$(mktemp -d)
trap 'rm -rf -- "$tmp"' EXIT
if [[ $bundle == arch ]]; then
    command -v pacman >/dev/null || { echo 'Use this bundle on Arch/Omarchy.'; exit 1; }
    mkdir -p "$tmp/db/sync" "$tmp/cache"
    cp -a /var/lib/pacman/local "$tmp/db/"
    cp packages/sni-offline.db.tar.gz "$tmp/db/sync/sni-offline.db"
    cat > "$tmp/pacman.conf" <<CONF
[options]
Architecture = x86_64
DBPath = $tmp/db
CacheDir = $tmp/cache
LogFile = $tmp/pacman.log
SigLevel = Optional
[sni-offline]
Server = file://$PWD/packages
CONF
    # Resolve against the destination machine, not the build machine.
    pacman --config "$tmp/pacman.conf" -Sp --needed --noconfirm --print-format '%l' sni-tunnel-v2-bin hyprpolkitagent wl-clipboard xclip ttf-dejavu > "$tmp/urls"
    mapfile -t urls < "$tmp/urls"
    ((${#urls[@]})) || { echo 'Already installed, or dependency resolution failed. Check the output above.'; exit 1; }
    packages=()
    for url in "${urls[@]}"; do
        file=${url#file://}
        [[ -f $file ]] || { echo "Missing bundled package: $file"; exit 1; }
        read -r name version < <(pacman -Qp "$file")
        if installed=$(pacman -Q "$name" 2>/dev/null); then
            old=${installed#* }
            if [[ $name != sni-tunnel-v2-bin && $old != "$version" ]]; then
                echo "This snapshot requires $name $version; your system has $old."
                echo 'Update the complete Omarchy/Arch system first, then rerun. No partial system upgrade was performed.'
                exit 1
            fi
        fi
        packages+=("$file")
    done
    sudo pacman -U --needed "${packages[@]}"
    if ! pgrep -u "$UID" -f 'hyprpolkitagent|polkit-gnome-authentication-agent|polkit-kde-authentication-agent|lxqt-policykit|mate-polkit' >/dev/null; then
        systemctl --user enable --now hyprpolkitagent.service
    fi
elif [[ $bundle == ubuntu-24.04 ]]; then
    [[ $ID == ubuntu && ${VERSION_ID:-} == 24.04 ]] || { echo 'This bundle targets Ubuntu 24.04 x86_64. Use the matching distribution bundle.'; exit 1; }
    chmod 755 "$tmp"
    mkdir -p "$tmp/lists/partial"
    printf 'deb [trusted=yes] file:%s/packages ./\n' "$PWD" > "$tmp/sources.list"
    opts=(-o "Dir::Etc::sourcelist=$tmp/sources.list" -o Dir::Etc::sourceparts=- -o "Dir::State::lists=$tmp/lists" -o "Dir::Cache::archives=$PWD/packages" -o APT::Get::List-Cleanup=0)
    sudo apt-get "${opts[@]}" update
    sudo apt-get "${opts[@]}" --no-download install sni-tunnel-v2 policykit-1-gnome wl-clipboard xclip fonts-dejavu-core
    if ! pgrep -u "$UID" -f 'polkit.*authentication|hyprpolkitagent|gnome-shell|lxqt-policykit|mate-polkit' >/dev/null; then
        mkdir -p "${XDG_CONFIG_HOME:-$HOME/.config}/autostart"
        cat > "${XDG_CONFIG_HOME:-$HOME/.config}/autostart/sni-polkit-agent.desktop" <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=Authentication agent
Exec=/usr/lib/policykit-1-gnome/polkit-gnome-authentication-agent-1
NoDisplay=true
DESKTOP
        nohup /usr/lib/policykit-1-gnome/polkit-gnome-authentication-agent-1 > "$tmp/agent.log" 2>&1 < /dev/null &
    fi
else
    echo 'Unknown bundle platform'; exit 1
fi
echo 'Installed. Open SNI Tunnel V2 from the application menu. Enable TUN in Settings and save before connecting.'
