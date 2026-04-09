#!/bin/sh
set -e

LIBHEIF_VERSION=1.21.2
PREFIX="$HOME/.local"

if ! command -v pkg-config >/dev/null 2>&1; then
	echo "Installing pkgconf..."
	brew install pkgconf
fi

if ! brew list libde265 >/dev/null 2>&1; then
	echo "Installing libde265..."
	brew install libde265
fi

if [ ! -f "$PREFIX/lib/libheif.dylib" ]; then
	echo "Building libheif $LIBHEIF_VERSION (decode-only, linked to libde265)..."
	cd /tmp
	rm -rf libheif-build
	git clone --depth 1 --branch "v$LIBHEIF_VERSION" https://github.com/strukturag/libheif.git libheif-build
	cd libheif-build
	mkdir build && cd build
	cmake .. \
		-DCMAKE_INSTALL_PREFIX="$PREFIX" \
		-DBUILD_SHARED_LIBS=ON \
		-DWITH_EXAMPLES=OFF \
		-DWITH_GDK_PIXBUF=OFF \
		-DWITH_X265=OFF \
		-DWITH_AOM_DECODER=OFF \
		-DWITH_AOM_ENCODER=OFF \
		-DWITH_RAV1E=OFF \
		-DWITH_DAV1D=OFF \
		-DWITH_JPEG_DECODER=OFF \
		-DWITH_JPEG_ENCODER=OFF \
		-DWITH_OpenJPEG_DECODER=OFF \
		-DWITH_OpenJPEG_ENCODER=OFF \
		-DWITH_KVAZAAR=OFF \
		-DWITH_FFMPEG_DECODER=OFF \
		-DWITH_VVDEC=OFF \
		-DWITH_VVENC=OFF \
		-DWITH_UNCOMPRESSED_CODEC=OFF \
		-DENABLE_PLUGIN_LOADING=OFF
	make -j"$(sysctl -n hw.ncpu)"
	make install
	rm -rf /tmp/libheif-build
fi

export PKG_CONFIG_PATH="$PREFIX/lib/pkgconfig:$(brew --prefix libde265)/lib/pkgconfig"
cargo install --path .
install_name_tool -add_rpath "$PREFIX/lib" "$(which img2avif)"

echo "Done. img2avif installed to $(which img2avif)"
