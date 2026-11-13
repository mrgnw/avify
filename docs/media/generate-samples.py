# /// script
# requires-python = ">=3.14"
# dependencies = ["pillow==12.2.0"]
# ///

import argparse
import math
import random
from pathlib import Path

from PIL import Image

parser = argparse.ArgumentParser(description='Generate synthetic avify demo images.')
parser.add_argument('output', type=Path)
output = parser.parse_args().output
output.mkdir(parents=True, exist_ok=True)
for name in ['landscape.jpg', 'texture.png', 'gradient.png']:
	if (output / name).exists():
		parser.error(f'{output / name} already exists; choose an empty directory')
width, height = 960, 640
rng = random.Random(42)

landscape = Image.new('RGB', (width, height))
pixels = landscape.load()
for y in range(height):
	for x in range(width):
		grain = rng.randrange(-12, 13)
		ridge = 340 + 70 * math.sin(x / 130) + 30 * math.sin(x / 43)
		if y < ridge:
			color = (110 + y // 8, 160 + y // 14, 210 + y // 24)
		else:
			color = (60 + y // 16, 105 + y // 22, 65 + y // 30)
		pixels[x, y] = tuple(max(0, min(255, value + grain)) for value in color)
landscape.save(output / 'landscape.jpg', quality=95, subsampling=0)
landscape.save(output / 'texture.png')

gradient = Image.new('RGB', (width, height))
pixels = gradient.load()
for y in range(height):
	for x in range(width):
		pixels[x, y] = (x * 255 // (width - 1), y * 255 // (height - 1), (x + y) % 256)
gradient.save(output / 'gradient.png')

for sample in sorted(output.iterdir()):
	if sample.suffix in {'.jpg', '.png'}:
		print(f'{sample.name}: {sample.stat().st_size} bytes')
