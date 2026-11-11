_pkgname=virtual-network
pkgname=virtual-network-git
pkgver=r0.000000
pkgrel=1
pkgdesc="Virtual network written in Rust for Arch Linux"
arch=('x86_64')
url="https://github.com/Student-Team-Projects/virtual-network"
license=('GPL-3.0-or-later')
depends=('gcc-libs')
makedepends=('cargo' 'git')
provides=("${_pkgname}")
conflicts=("${_pkgname}")
source=("${_pkgname}::git+${url}.git")
sha256sums=('SKIP')

pkgver() {
  cd "${_pkgname}"
  printf "r%s.%s" "$(git rev-list --count HEAD)" "$(git rev-parse --short HEAD)"
}

prepare() {
  cd "${_pkgname}"
  export RUSTUP_TOOLCHAIN=stable
  cargo fetch --locked --target "$(rustc -vV | sed -n 's/host: //p')"
}

build() {
  cd "${_pkgname}"
  export RUSTUP_TOOLCHAIN=stable
  export CARGO_TARGET_DIR=target
  cargo build --release --frozen
}

check() {
  cd "${_pkgname}"
  export RUSTUP_TOOLCHAIN=stable
  export CARGO_TARGET_DIR=target
  cargo test --frozen
}

package() {
  cd "${_pkgname}"
  install -Dm755 "target/release/virtual_network_daemon" "${pkgdir}/usr/bin/virtual_network_daemon"
  install -Dm644 "virtual_network.service" "${pkgdir}/usr/lib/systemd/system/virtual_network.service"
}
