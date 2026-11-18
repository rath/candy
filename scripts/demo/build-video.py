#!/usr/bin/env python3
"""Builds the landing-page demo assets from record-demo.sh output.

For each language, joins WORK_DIR/raw/{chat,sheet,order}.<lang>.mov into one muted H.264 loop,
run about 1.9 times faster than recorded, with crossfades between the scenes. The loop opens on
the last frame of the order page, so it starts where it ends. WORK_DIR/raw/still.<lang>.png, the
finished order-page window, becomes the poster and reduced-motion still as PNG and WebP.

    python3 scripts/demo/build-video.py [WORK_DIR] [--langs en,ko,ja] [--out DIR]

Writes site/assets/candy-demo-<lang>.mp4 and candy-editor-<lang>.{png,webp}. Needs ffmpeg,
ffprobe and ImageMagick's magick.
"""

import argparse
import pathlib
import subprocess

ROOT = pathlib.Path(__file__).resolve().parents[2]
ASSETS = ROOT / "site" / "assets"
WIDTH, HEIGHT = 1280, 1100
SPEED = 1.9
FPS = 30
FADE = 0.5  # crossfade between scenes, seconds of output
OPENING = 0.5  # the finished order page before the first crossfade
HOLD = 0.6  # each scene's last frame, held before the next crossfade
CRF = 27


def run(*args: str) -> str:
    return subprocess.run(args, check=True, capture_output=True, text=True).stdout


def duration(path: pathlib.Path) -> float:
    return float(run("ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", str(path)))


def build(work: pathlib.Path, lang: str, assets: pathlib.Path) -> None:
    raw = work / "raw"
    clips = [raw / f"{scene}.{lang}.mov" for scene in ("chat", "sheet", "order")]
    opening = raw / f"opening.{lang}.png"
    # The loop starts on the frame it ends on: the order page after the pointer has left.
    subprocess.run(
        ["ffmpeg", "-v", "error", "-y", "-sseof", "-0.05", "-i", str(clips[-1]), "-frames:v", "1", "-update", "1", str(opening)],
        check=True,
    )

    scale = f"scale={WIDTH}:{HEIGHT}:flags=lanczos:out_color_matrix=bt709:out_range=tv"
    parts = [f"[0:v]trim=duration={OPENING + FADE},fps={FPS},{scale},format=yuv420p,setsar=1,settb=1/{FPS}[s0]"]
    lengths = [OPENING + FADE]
    for i, clip in enumerate(clips, start=1):
        length = duration(clip) / SPEED + HOLD
        parts.append(
            f"[{i}:v]setpts=(PTS-STARTPTS)/{SPEED},fps={FPS},tpad=stop_mode=clone:stop_duration={HOLD},"
            f"{scale},format=yuv420p,setsar=1,settb=1/{FPS},trim=duration={length:.3f}[s{i}]"
        )
        lengths.append(length)
    # Chain the crossfades; each starts FADE before the end of everything joined so far.
    label, total = "s0", lengths[0]
    for i in range(1, len(clips) + 1):
        out = f"x{i}"
        parts.append(f"[{label}][s{i}]xfade=transition=fade:duration={FADE}:offset={total - FADE:.3f}[{out}]")
        label, total = out, total + lengths[i] - FADE
    parts.append(f"[{label}]format=yuv420p[v]")

    video = assets / f"candy-demo-{lang}.mp4"
    subprocess.run(
        ["ffmpeg", "-v", "error", "-y", "-loop", "1", "-i", str(opening)]
        + [arg for clip in clips for arg in ("-i", str(clip))]
        + ["-filter_complex", ";".join(parts), "-map", "[v]", "-an",
           "-c:v", "libx264", "-preset", "veryslow", "-tune", "stillimage", "-crf", str(CRF),
           "-profile:v", "high", "-pix_fmt", "yuv420p",
           "-colorspace", "bt709", "-color_primaries", "bt709", "-color_trc", "bt709", "-color_range", "tv",
           "-movflags", "+faststart", str(video)],
        check=True,
    )
    print(f"{video}: {duration(video):.1f} s, {video.stat().st_size / 1e6:.2f} MB")

    still = raw / f"still.{lang}.png"
    png = assets / f"candy-editor-{lang}.png"
    webp = assets / f"candy-editor-{lang}.webp"
    subprocess.run(["magick", str(still), "-filter", "Lanczos", "-resize", f"{WIDTH}x{HEIGHT}!", "-strip", str(png)], check=True)
    subprocess.run(["magick", str(png), "-quality", "88", "-define", "webp:alpha-quality=100", str(webp)], check=True)
    print(f"{png}, {webp}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("work", nargs="?", default=str(ROOT / ".impeccable" / "review" / "demo"))
    parser.add_argument("--langs", default="en,ko,ja")
    parser.add_argument("--out", default=str(ASSETS), help="where the assets go (default site/assets)")
    args = parser.parse_args()
    for lang in args.langs.split(","):
        build(pathlib.Path(args.work), lang, pathlib.Path(args.out))


if __name__ == "__main__":
    main()
