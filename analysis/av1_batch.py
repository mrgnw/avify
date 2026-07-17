# /// script
# requires-python = ">=3.14"
# ///
import json
import subprocess
import time
from pathlib import Path

FFMPEG = '/opt/homebrew/bin/ffmpeg'
FFPROBE = '/opt/homebrew/bin/ffprobe'
SRC_DIR = Path.home() / 'Library/Mobile Documents/com~apple~CloudDocs/screenshots'
OUT_DIR = Path('/tmp/av1_batch')
RESULTS = Path(__file__).parent / 'av1_batch_results.json'
VIDEO_EXTS = {'.mov', '.mp4', '.m4v', '.webm', '.mkv', '.avi'}
SVT_ARGS = ['-c:v', 'libsvtav1', '-crf', '32', '-preset', '10']


def probe(path):
	out = subprocess.run(
		[FFPROBE, '-v', 'quiet', '-print_format', 'json', '-show_format', '-show_streams', str(path)],
		capture_output=True,
		text=True,
		check=True,
	).stdout
	info = json.loads(out)
	v = next((s for s in info['streams'] if s['codec_type'] == 'video'), None)
	return {
		'codec': v['codec_name'] if v else None,
		'res': f'{v["width"]}x{v["height"]}' if v else None,
		'dur': float(info['format'].get('duration', 0)),
	}


def materialize(path):
	subprocess.run(['brctl', 'download', str(path)], check=True)
	for _ in range(600):
		if path.stat().st_size > 0:
			return
		time.sleep(0.5)
	raise TimeoutError(f'iCloud download timed out: {path.name}')


def main():
	OUT_DIR.mkdir(exist_ok=True)
	videos = sorted(p for p in SRC_DIR.iterdir() if p.suffix.lower() in VIDEO_EXTS and p.is_file())
	assert videos, f'no videos in {SRC_DIR}'
	print(f'{len(videos)} videos')

	results = []
	for i, src in enumerate(videos, 1):
		row = {'name': src.name}
		try:
			materialize(src)
			row |= probe(src)
			row['orig'] = src.stat().st_size
			dst = OUT_DIR / f'{src.stem}.av1.mp4'
			t0 = time.monotonic()
			r = subprocess.run(
				[FFMPEG, '-y', '-loglevel', 'error', '-i', str(src), *SVT_ARGS, '-c:a', 'copy', str(dst)],
				capture_output=True,
				text=True,
			)
			row['time'] = round(time.monotonic() - t0, 2)
			if r.returncode != 0:
				row['error'] = r.stderr.strip()[:300]
			else:
				row['av1'] = dst.stat().st_size
		except Exception as e:
			row['error'] = str(e)[:300]
		results.append(row)
		RESULTS.write_text(json.dumps(results, indent=1))
		status = (
			f'{row["av1"] / 1e6:.1f}MB in {row["time"]}s'
			if 'av1' in row
			else f'ERROR: {row.get("error", "?")[:80]}'
		)
		print(f'[{i}/{len(videos)}] {src.name}: {row.get("orig", 0) / 1e6:.1f}MB → {status}', flush=True)

	ok = [r for r in results if 'av1' in r]
	errs = [r for r in results if 'error' in r]
	orig_t = sum(r['orig'] for r in ok)
	av1_t = sum(r['av1'] for r in ok)
	enc_t = sum(r['time'] for r in ok)
	dur_t = sum(r['dur'] for r in ok)
	print(f'\n{len(ok)} ok, {len(errs)} failed')
	print(f'{orig_t / 1e6:.0f}MB → {av1_t / 1e6:.0f}MB ({100 * (orig_t - av1_t) / orig_t:.0f}% saved)')
	print(f'{dur_t:.0f}s of video encoded in {enc_t:.0f}s ({dur_t / enc_t:.1f}x realtime)')


if __name__ == '__main__':
	main()
