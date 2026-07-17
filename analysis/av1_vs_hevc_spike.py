# /// script
# requires-python = ">=3.14"
# ///
import json
import shutil
import subprocess
import time
from pathlib import Path

FFMPEG = '/opt/homebrew/bin/ffmpeg'
FFPROBE = '/opt/homebrew/bin/ffprobe'
SRC_DIR = Path.home() / 'Library/Mobile Documents/com~apple~CloudDocs/screenshots'
WORK = Path('/tmp/av1_hevc_spike')

FILE_GLOBS = [
	'Screen Recording 2026-04-28*.mov',
	'Screen Recording 2026-07-08*.mov',
	'IMG_2272.MOV',
	'Screen Recording 2026-07-06*.mov',
	'Screen Recording 2026-07-16*.mov',
]

ENCODERS = {
	'hevc_vt': ['-c:v', 'hevc_videotoolbox', '-q:v', '65', '-tag:v', 'hvc1'],
	'av1_svt_p8': ['-c:v', 'libsvtav1', '-crf', '32', '-preset', '8'],
	'av1_svt_p10': ['-c:v', 'libsvtav1', '-crf', '32', '-preset', '10'],
}


def probe(path):
	out = subprocess.run(
		[FFPROBE, '-v', 'quiet', '-print_format', 'json', '-show_format', '-show_streams', str(path)],
		capture_output=True,
		text=True,
		check=True,
	).stdout
	info = json.loads(out)
	v = next(s for s in info['streams'] if s['codec_type'] == 'video')
	return {
		'codec': v['codec_name'],
		'res': f'{v["width"]}x{v["height"]}',
		'dur': float(info['format']['duration']),
	}


def encode(src, dst, vargs):
	t0 = time.monotonic()
	r = subprocess.run(
		[FFMPEG, '-y', '-loglevel', 'error', '-i', str(src), *vargs, '-c:a', 'copy', str(dst)],
		capture_output=True,
		text=True,
	)
	elapsed = time.monotonic() - t0
	if r.returncode != 0:
		return None, r.stderr.strip()
	return elapsed, dst.stat().st_size


def main():
	WORK.mkdir(exist_ok=True)
	results = []
	for pattern in FILE_GLOBS:
		matches = sorted(SRC_DIR.glob(pattern))
		assert matches, f'no match: {pattern}'
		src = matches[0]
		name = src.name
		subprocess.run(['brctl', 'download', str(src)], check=True)
		local = WORK / name
		if not local.exists():
			shutil.copy2(src, local)
		info = probe(local)
		row = {'name': name, 'orig': local.stat().st_size, **info}
		for label, vargs in ENCODERS.items():
			dst = WORK / f'{local.stem}.{label}.mp4'
			elapsed, size_or_err = encode(local, dst, vargs)
			if elapsed is None:
				print(f'{name} {label} FAILED: {size_or_err}')
				row[label] = None
			else:
				row[label] = {'time': elapsed, 'size': size_or_err}
			print(f'{name} {label}: {row[label]}')
		results.append(row)

	(WORK / 'results.json').write_text(json.dumps(results, indent=1))

	print('\n=== SUMMARY ===')
	hdr = f'{"file":<44} {"dur":>5} {"orig MB":>8}'
	for label in ENCODERS:
		hdr += f' | {label + " MB":>14} {label + " s":>10}'
	print(hdr)
	for r in results:
		line = f'{r["name"][:44]:<44} {r["dur"]:>4.0f}s {r["orig"] / 1e6:>7.1f}'
		for label in ENCODERS:
			e = r[label]
			if e:
				line += f' | {e["size"] / 1e6:>13.1f} {e["time"]:>9.1f}'
			else:
				line += f' | {"FAIL":>13} {"-":>9}'
		print(line)


if __name__ == '__main__':
	main()
