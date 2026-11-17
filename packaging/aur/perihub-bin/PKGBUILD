# Maintainer: lamasniper7654 <119895168+hex-dump@users.noreply.github.com>

pkgname=perihub-bin
_pkgname=perihub
pkgver=0.9.2
pkgrel=1
pkgdesc='One app for gaming mice and keyboards across brands, instead of G HUB, GG, iCUE and Synapse'
arch=('x86_64')
url='https://github.com/hex-dump/perihub'
license=('MIT')
depends=('cairo' 'dbus' 'gcc-libs' 'gdk-pixbuf2' 'glib2' 'glibc' 'gtk3' 'hicolor-icon-theme'
         'libayatana-appindicator' 'libsoup3' 'libxkbcommon' 'systemd-libs' 'webkit2gtk-4.1')
provides=("$_pkgname")
conflicts=("$_pkgname")
options=('!strip' '!debug')
source=("https://github.com/hex-dump/perihub/releases/download/v$pkgver/PeriHub_${pkgver}_amd64.deb"
        "LICENSE-$pkgver::https://raw.githubusercontent.com/hex-dump/perihub/v$pkgver/LICENSE")
sha256sums=('9796def1f10edf8035a1b99540e55e07b1cafdff59a707a6131c61b36067381c'
            '440e353d7bb443f50c947eacbc5a437d4707c6f0595b7fa509d530bab1aef67f')

package() {
  # The .deb contains the binary, the desktop file, the icons and the udev rule.
  bsdtar -xf data.tar.* -C "$pkgdir"
  install -Dm644 "LICENSE-$pkgver" "$pkgdir/usr/share/licenses/$pkgname/LICENSE"
}
