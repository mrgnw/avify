# Media credits

The benchmark photos are not stored in this repository. `benchmark.py` downloads them from these URLs and checks their SHA-256 sums. None show people; `exiftool` found no GPS coordinates in any of them.

| File in the benchmark | Source | Author | Licence | SHA-256 of the download |
|-|-|-|-|-|
| `bear-mountain.jpg` (5184×3456 JPEG, quality about 98) | [Bear Mountain covered in snow](https://commons.wikimedia.org/wiki/File:Bear_Mountain_covered_in_snow_(24009252743).jpg), Wikimedia Commons | [Thomson20192](https://www.flickr.com/people/93558439@N05) | [CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/) | `92932f9482925d3fa42fe17d16451d28f7a06ef84ffa586995fd61314e94e8d0` |
| `rocks.dng` (Pentax K-S1 DNG) | [raw.pixls.us sample 1365](https://raw.pixls.us/getfile.php/1365/nice/Pentax%20-%20K-S1%20-%2012bit%20(3:2).DNG) | raw.pixls.us contributor (no artist in EXIF) | [CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/) | `58abc43a8e025dcab0053bcee173d20197aa2b7983e5e62097147a1e8545f111` |
| `tulips.png` (PNG export of a Pentax K-3 II DNG) | [raw.pixls.us sample 1363](https://raw.pixls.us/getfile.php/1363/nice/Pentax%20-%20K-3%20II%20-%2014bit%20(3:2).DNG) | Christian Kanzian (EXIF artist) | [CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/) | `f43b9dc35e4d1b43918d2d4bd0973bc202af48ed24c924e4aff61250c971b171` |

`tulips.png` is made by `benchmark.py`: LibRaw (through rawpy 0.27.0) develops the DNG with the camera white balance into an 8-bit sRGB image, and Pillow saves it as PNG at its default compression level.

`hero.gif`, `results.png`, `quality-crops.png` and `social-preview.png` are rendered by `render.py` from these photos and `results.json`. They are released under this repository's [MIT licence](../../LICENSE); the photo crops inside them stay CC0.
