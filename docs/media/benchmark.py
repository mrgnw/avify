# /// script
# requires-python = ">=3.14"
# dependencies = ["pillow==12.2.0", "rawpy==0.27.0", "scikit-image==0.26.0", "numpy==2.3.4"]
# ///

import argparse
import hashlib
import json
import shutil
import subprocess
import time
import urllib.request
from pathlib import Path

import numpy as np
import rawpy
from PIL import Image
from skimage.metrics import structural_similarity

SAMPLES = [
	{
		'name': 'bear-mountain.jpg',
		'kind': 'JPEG',
		'url': 'https://upload.wikimedia.org/wikipedia/commons/5/5f/Bear_Mountain_covered_in_snow_%2824009252743%29.jpg',
		'sha256': '92932f9482925d3fa42fe17d16451d28f7a06ef84ffa586995fd61314e94e8d0',
	},
	{
		'name': 'rocks.dng',
		'kind': 'RAW',
		'url': 'https://raw.pixls.us/getfile.php/1365/nice/Pentax%20-%20K-S1%20-%2012bit%20(3:2).DNG',
		'sha256': '58abc43a8e025dcab0053bcee173d20197aa2b7983e5e62097147a1e8545f111',
	},
	{
		'name': 'tulips.png',
		'kind': 'PNG',
		'url': 'https://raw.pixls.us/getfile.php/1363/nice/Pentax%20-%20K-3%20II%20-%2014bit%20(3:2).DNG',
		'sha256': 'f43b9dc35e4d1b43918d2d4bd0973bc202af48ed24c924e4aff61250c971b171',
	},
]
USER_AGENT = 'avify-benchmark/1.0 (https://github.com/mrgnw/avify)'


def download(url: str, path: Path) -> None:
	request = urllib.request.Request(url, headers={'User-Agent': USER_AGENT})
	with urllib.request.urlopen(request, timeout=120) as response, path.open('wb') as out:
		shutil.copyfileobj(response, out)


def fetch(sample: dict, cache: Path) -> Path:
	suffix = Path(sample['url']).suffix.lower()
	source = cache / f'source-{Path(sample["name"]).stem}{suffix}'
	if not source.exists():
		download(sample['url'], source)
	digest = hashlib.sha256(source.read_bytes()).hexdigest()
	assert digest == sample['sha256'], f'{source} checksum {digest} does not match'
	return source


def export_png(raw_path: Path, png_path: Path) -> None:
	with rawpy.imread(str(raw_path)) as raw:
		rgb = raw.postprocess(use_camera_wb=True, output_bps=8)
	Image.fromarray(rgb).save(png_path)


def prepare(workdir: Path, raw_reference: str) -> Path:
	cache = workdir / 'cache'
	photos = workdir / 'photos'
	references = workdir / 'reference'
	for folder in (cache, photos, references):
		folder.mkdir(parents=True, exist_ok=True)
	for sample in SAMPLES:
		source = fetch(sample, cache)
		photo = photos / sample['name']
		reference = references / (photo.stem + '.png')
		if not photo.exists():
			if sample['kind'] == 'PNG':
				export_png(source, photo)
			else:
				shutil.copyfile(source, photo)
		if not reference.exists():
			if sample['kind'] == 'RAW':
				subprocess.run([raw_reference, str(photo), str(reference)], check=True)
			else:
				Image.open(photo).convert('RGB').save(reference, compress_level=1)
	return photos


def ssimulacra2(binary: str, reference: Path, decoded: Path) -> float:
	output = subprocess.run(
		[binary, 'image', str(reference), str(decoded)], capture_output=True, text=True, check=True
	).stdout
	return round(float(output.split()[-1]), 1)


def measure(args: argparse.Namespace, photos: Path, quality: int) -> dict:
	outdir = args.workdir / f'avif-q{quality}-s{args.speed}'
	if outdir.exists():
		shutil.rmtree(outdir)
	flags = ['--keep', '--quiet', '--quality', str(quality), '--speed', str(args.speed)]
	started = time.monotonic()
	subprocess.run([args.avify, *flags, '--outdir', str(outdir), str(photos)], check=True)
	seconds = round(time.monotonic() - started, 2)

	rows = []
	for sample in SAMPLES:
		photo = photos / sample['name']
		avif = outdir / (photo.stem + '.avif')
		assert avif.exists(), f'avify kept {photo.name} instead of writing {avif.name}'
		reference = args.workdir / 'reference' / (photo.stem + '.png')
		decoded = outdir / (photo.stem + '.decoded.png')
		Image.open(avif).convert('RGB').save(decoded, compress_level=1)
		a = np.asarray(Image.open(reference))
		b = np.asarray(Image.open(decoded))
		assert a.shape == b.shape, f'{reference} {a.shape} vs {decoded} {b.shape}'
		row = {
			'name': sample['name'],
			'kind': sample['kind'],
			'width': a.shape[1],
			'height': a.shape[0],
			'input_bytes': photo.stat().st_size,
			'avif_bytes': avif.stat().st_size,
			'ssimulacra2': ssimulacra2(args.ssimulacra2, reference, decoded),
			'ssim': round(float(structural_similarity(a, b, channel_axis=2)), 3),
		}
		row['ratio'] = round(row['input_bytes'] / row['avif_bytes'], 1)
		rows.append(row)
	total_input = sum(row['input_bytes'] for row in rows)
	total_avif = sum(row['avif_bytes'] for row in rows)
	return {
		'quality': quality,
		'speed': args.speed,
		'command': f'avify {" ".join(flags)} --outdir OUTPUT_DIR PHOTO_DIR',
		'seconds': seconds,
		'total_input_bytes': total_input,
		'total_avif_bytes': total_avif,
		'total_ratio': round(total_input / total_avif, 1),
		'rows': rows,
	}


def report(run: dict) -> None:
	print(run['command'])
	for row in run['rows']:
		print(
			f'  {row["kind"]:<5} {row["input_bytes"]:>12,} -> {row["avif_bytes"]:>10,} bytes '
			f'{row["ratio"]:>5}x  SSIMULACRA2 {row["ssimulacra2"]:>5}  SSIM {row["ssim"]}'
		)
	print(
		f'  folder {run["total_input_bytes"]:>11,} -> {run["total_avif_bytes"]:>10,} bytes '
		f'{run["total_ratio"]:>5}x  in {run["seconds"]} s'
	)


def main() -> None:
	parser = argparse.ArgumentParser(description='Measure avify on the credited sample photos.')
	parser.add_argument('workdir', type=Path, help='directory for downloads and outputs; reused as a cache')
	parser.add_argument('--avify', default='avify')
	parser.add_argument('--raw-reference', required=True, help='path to the raw_reference example binary')
	parser.add_argument('--ssimulacra2', default='ssimulacra2_rs')
	parser.add_argument('--quality', type=int, nargs='+', default=[80, 90])
	parser.add_argument('--speed', type=int, default=10)
	parser.add_argument('--json', type=Path, help='write results here')
	args = parser.parse_args()

	photos = prepare(args.workdir, args.raw_reference)
	version = subprocess.run([args.avify, '--version'], capture_output=True, text=True, check=True).stdout
	runs = [measure(args, photos, quality) for quality in args.quality]
	print(version.strip())
	for run in runs:
		report(run)
	if args.json:
		results = {'avify_version': version.strip(), 'runs': runs}
		args.json.write_text(json.dumps(results, indent='\t') + '\n')


main()
