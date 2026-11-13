# /// script
# requires-python = ">=3.14"
# dependencies = ["pillow==12.2.0", "numpy==2.3.4"]
# ///

import argparse
import json
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFont

BG = (13, 17, 23)
PANEL = (22, 27, 34)
TEXT = (230, 237, 243)
MUTED = (139, 148, 158)
ORIGINAL = (110, 118, 129)
AVIF = (63, 185, 80)
ACCENT = (255, 166, 87)
SANS = '/System/Library/Fonts/HelveticaNeue.ttc'
MONO = '/System/Library/Fonts/Menlo.ttc'
CROPS = {
	'rocks.dng': (3950, 2150),
	'bear-mountain.jpg': (1600, 950),
	'tulips.png': (3150, 1050),
}
HERO_CROP = ('rocks.dng', 3600, 2120, 840, 330)
GIF_SIZE = (880, 480)


def font(path: str, size: int, index: int = 0) -> ImageFont.FreeTypeFont:
	return ImageFont.truetype(path, size, index=index)


def megabytes(n: int) -> str:
	return f'{n / 1_000_000:.1f} MB' if n >= 1_000_000 else f'{n / 1_000_000:.2f} MB'


def crop(path: Path, x: int, y: int, w: int, h: int) -> Image.Image:
	return Image.open(path).convert('RGB').crop((x, y, x + w, y + h))


def pair(workdir: Path, run: dict, name: str, box: tuple) -> tuple[Image.Image, Image.Image]:
	stem = Path(name).stem
	outdir = workdir / f'avif-q{run["quality"]}-s{run["speed"]}'
	return crop(workdir / 'reference' / f'{stem}.png', *box), crop(outdir / f'{stem}.decoded.png', *box)


def difference(a: Image.Image, b: Image.Image, gain: int) -> Image.Image:
	delta = np.abs(np.asarray(a).astype(int) - np.asarray(b).astype(int)) * gain
	return Image.fromarray(np.clip(delta, 0, 255).astype('uint8'))


def results_chart(run: dict, out: Path) -> None:
	rows = sorted(run['rows'], key=lambda row: -row['input_bytes'])
	width, row_height, left = 880, 74, 210
	image = Image.new('RGB', (width, 110 + row_height * len(rows) + 40), BG)
	draw = ImageDraw.Draw(image)
	draw.text(
		(32, 26),
		f'avify at default settings (quality {run["quality"]}, speed {run["speed"]})',
		TEXT,
		font(SANS, 24, 1),
	)
	draw.text((32, 60), 'File size per photo, original vs AVIF. Bars share one scale.', MUTED, font(SANS, 16))
	scale = (width - left - 220) / max(row['input_bytes'] for row in rows)
	for i, row in enumerate(rows):
		y = 110 + i * row_height
		draw.text((32, y + 4), row['kind'], TEXT, font(SANS, 20, 1))
		draw.text((32, y + 30), row['name'], MUTED, font(MONO, 13))
		draw.rectangle((left, y, left + row['input_bytes'] * scale, y + 22), ORIGINAL)
		draw.text(
			(left + row['input_bytes'] * scale + 8, y + 2),
			megabytes(row['input_bytes']),
			MUTED,
			font(SANS, 16),
		)
		draw.rectangle((left, y + 28, left + max(2, row['avif_bytes'] * scale), y + 50), AVIF)
		draw.text(
			(left + row['avif_bytes'] * scale + 8, y + 30), megabytes(row['avif_bytes']), AVIF, font(SANS, 16)
		)
		draw.text((width - 32, y + 10), f'{row["ratio"]}x', ACCENT, font(SANS, 30, 1), anchor='ra')
	y = 110 + row_height * len(rows) + 6
	draw.text(
		(32, y),
		f'Folder: {megabytes(run["total_input_bytes"])} to {megabytes(run["total_avif_bytes"])}, {run["total_ratio"]}x smaller',
		TEXT,
		font(SANS, 16),
	)
	image.save(out, optimize=True)


def crop_comparison(workdir: Path, run: dict, out: Path) -> None:
	w, h, gap, label = 280, 210, 10, 30
	rows = {row['name']: row for row in run['rows']}
	image = Image.new('RGB', (3 * w + 2 * gap, len(CROPS) * (h + label + gap)), BG)
	draw = ImageDraw.Draw(image)
	for i, (name, (x, y)) in enumerate(CROPS.items()):
		top = i * (h + label + gap)
		original, avif = pair(workdir, run, name, (x, y, w, h))
		row = rows[name]
		titles = [
			f'{row["kind"]} original, {megabytes(row["input_bytes"])}'
			if row['kind'] != 'RAW'
			else f'RAW as decoded, {megabytes(row["input_bytes"])}',
			f'AVIF q{run["quality"]}, {megabytes(row["avif_bytes"])}',
			'difference x8',
		]
		for j, tile in enumerate([original, avif, difference(original, avif, 8)]):
			image.paste(tile, (j * (w + gap), top + label))
			draw.text((j * (w + gap) + 2, top + 6), titles[j], TEXT if j < 2 else MUTED, font(SANS, 15))
	image.save(out, optimize=True)


def ease(t: float) -> float:
	return 1 - (1 - t) ** 3


def folder_frame(run: dict, progress: float, command: str, summary: str) -> Image.Image:
	image = Image.new('RGB', GIF_SIZE, BG)
	draw = ImageDraw.Draw(image)
	mono, small = font(MONO, 18), font(MONO, 15)
	draw.text((32, 24), '~/photos', MUTED, small)
	rows = run['rows']
	scale = 420 / max(row['input_bytes'] for row in rows)
	for i, row in enumerate(rows):
		y = 60 + i * 44
		size = row['input_bytes'] + (row['avif_bytes'] - row['input_bytes']) * progress
		name = row['name'] if progress < 1 else Path(row['name']).stem + '.avif'
		color = AVIF if progress >= 1 else TEXT
		draw.text((32, y), name, color, mono)
		draw.rectangle((260, y + 4, 260 + max(2, size * scale), y + 20), AVIF if progress > 0 else ORIGINAL)
		draw.text((GIF_SIZE[0] - 32, y), megabytes(int(size)), color, mono, anchor='ra')
	total = run['total_input_bytes'] + (run['total_avif_bytes'] - run['total_input_bytes']) * progress
	draw.line((32, 196, GIF_SIZE[0] - 32, 196), PANEL, 2)
	draw.text((32, 208), 'total', MUTED, mono)
	draw.text(
		(GIF_SIZE[0] - 32, 204),
		megabytes(int(total)),
		AVIF if progress > 0 else TEXT,
		font(SANS, 30, 1),
		anchor='ra',
	)
	draw.rectangle((24, 270, GIF_SIZE[0] - 24, 340), PANEL)
	draw.text((40, 284), '$ ' + command, TEXT, mono)
	draw.text((40, 312), summary, AVIF, small)
	if progress >= 1:
		draw.text((32, 368), f'{run["total_ratio"]}x smaller', ACCENT, font(SANS, 64, 1))
		ratios = '   '.join(
			f'{row["kind"]} {row["ratio"]}x' for row in sorted(rows, key=lambda row: -row['ratio'])
		)
		draw.text((GIF_SIZE[0] - 32, 400), ratios, MUTED, font(SANS, 18), anchor='ra')
		draw.text(
			(GIF_SIZE[0] - 32, 428),
			f'default settings, quality {run["quality"]}',
			MUTED,
			font(SANS, 15),
			anchor='ra',
		)
	return image


def sweep_frame(original: Image.Image, avif: Image.Image, run: dict, row: dict, split: float) -> Image.Image:
	image = Image.new('RGB', GIF_SIZE, BG)
	draw = ImageDraw.Draw(image)
	x0, y0 = 20, 70
	cut = int(original.width * split)
	image.paste(original.crop((0, 0, cut, original.height)), (x0, y0))
	image.paste(avif.crop((cut, 0, avif.width, avif.height)), (x0 + cut, y0))
	draw.line((x0 + cut, y0 - 6, x0 + cut, y0 + original.height + 6), TEXT, 2)
	draw.text((20, 18), '100% crop, no scaling', TEXT, font(SANS, 22, 1))
	draw.text(
		(GIF_SIZE[0] - 20, 22),
		f'SSIMULACRA2 {row["ssimulacra2"]} (70 = high quality, 90 = visually lossless)',
		MUTED,
		font(SANS, 15),
		anchor='ra',
	)
	label_y = y0 + original.height + 14
	draw.text(
		(x0, label_y),
		f'original {row["kind"]} (as decoded), {megabytes(row["input_bytes"])}',
		TEXT,
		font(SANS, 18),
	)
	draw.text(
		(x0 + original.width, label_y),
		f'AVIF q{run["quality"]}, {megabytes(row["avif_bytes"])}',
		AVIF,
		font(SANS, 18),
		anchor='ra',
	)
	draw.text(
		(x0, label_y + 30),
		'The AVIF keeps the rock texture and edges; it smooths fine sensor grain.',
		MUTED,
		font(SANS, 15),
	)
	return image


def hero_gif(workdir: Path, run: dict, out: Path) -> None:
	name, *box = HERO_CROP
	row = next(row for row in run['rows'] if row['name'] == name)
	original, avif = pair(workdir, run, name, box)
	command = 'avify --keep --outdir avif ~/photos'
	frames, durations = [], []

	def add(image: Image.Image, ms: int) -> None:
		frames.append(image)
		durations.append(ms)

	add(folder_frame(run, 0, '', ''), 1600)
	for n in range(1, len(command) + 1, 3):
		add(folder_frame(run, 0, command[:n], ''), 60)
	add(folder_frame(run, 0, command, ''), 500)
	for step in range(1, 11):
		add(folder_frame(run, ease(step / 10) * 0.999, command, ''), 70)
	summary_frame = folder_frame(run, 1, command, f'{len(run["rows"])} AVIF files in avif/, originals kept')
	add(summary_frame, 3500)
	for step in range(0, 25):
		split = 0.5 + 0.42 * np.cos(np.pi * step / 12)
		add(sweep_frame(original, avif, run, row, split), 90)
	add(sweep_frame(original, avif, run, row, 0.5), 2500)
	sample = Image.new('RGB', (GIF_SIZE[0], GIF_SIZE[1] * 2))
	sample.paste(frames[-1], (0, 0))
	sample.paste(summary_frame, (0, GIF_SIZE[1]))
	palette = sample.quantize(colors=256, method=Image.Quantize.MEDIANCUT)
	indexed = [frame.quantize(palette=palette, dither=Image.Dither.NONE) for frame in frames]
	indexed[0].save(out, save_all=True, append_images=indexed[1:], duration=durations, loop=0, optimize=True)


def social_preview(workdir: Path, run: dict, out: Path) -> None:
	image = Image.new('RGB', (1280, 640), BG)
	draw = ImageDraw.Draw(image)
	name, x, y, _, _ = HERO_CROP
	row = next(row for row in run['rows'] if row['name'] == name)
	original, avif = pair(workdir, run, name, (x + 200, y, 560, 520))
	half = original.width // 2
	image.paste(original.crop((0, 0, half, original.height)), (680, 60))
	image.paste(avif.crop((half, 0, original.width, original.height)), (680 + half, 60))
	draw.line((680 + half, 50, 680 + half, 590), TEXT, 3)
	draw.text((690, 590), f'original {row["kind"]}  {megabytes(row["input_bytes"])}', TEXT, font(SANS, 20))
	draw.text((1230, 590), f'AVIF  {megabytes(row["avif_bytes"])}', AVIF, font(SANS, 20), anchor='ra')
	draw.text((60, 60), 'avify', TEXT, font(MONO, 44))
	draw.text((60, 140), f'{run["total_ratio"]}x', ACCENT, font(SANS, 170, 1))
	draw.text((64, 340), 'smaller photo folder', TEXT, font(SANS, 38, 1))
	for i, r in enumerate(sorted(run['rows'], key=lambda r: -r['ratio'])):
		draw.text((64, 410 + i * 40), f'{r["kind"]:<5}', MUTED, font(SANS, 26))
		draw.text(
			(200, 410 + i * 40),
			f'{megabytes(r["input_bytes"])} to {megabytes(r["avif_bytes"])}   {r["ratio"]}x',
			TEXT,
			font(SANS, 26),
		)
	draw.text(
		(64, 548), f'Measured at default quality {run["quality"]} on CC0 photos.', MUTED, font(SANS, 20)
	)
	draw.text((64, 578), '100% crop on the right: original | AVIF', MUTED, font(SANS, 20))
	image.save(out, optimize=True)


def main() -> None:
	parser = argparse.ArgumentParser(description='Render README media from benchmark.py results.')
	parser.add_argument('workdir', type=Path, help='the workdir benchmark.py used')
	parser.add_argument('--results', type=Path, default=Path(__file__).parent / 'results.json')
	parser.add_argument('--out', type=Path, default=Path(__file__).parent)
	args = parser.parse_args()
	run = json.loads(args.results.read_text())['runs'][0]
	assert run['quality'] == 80, 'the first run must use the default quality'
	results_chart(run, args.out / 'results.png')
	crop_comparison(args.workdir, run, args.out / 'quality-crops.png')
	social_preview(args.workdir, run, args.out / 'social-preview.png')
	hero_gif(args.workdir, run, args.out / 'hero.gif')


main()
