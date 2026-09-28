"""Regenerate the small supplementary fonts; source fonts are supplied explicitly.

Noto Emoji: https://github.com/google/fonts/tree/main/ofl/notoemoji
Noto CJK: https://github.com/notofonts/noto-cjk
Both use SIL OFL 1.1; licenses live beside the generated C files.
"""
import argparse
import pathlib
import subprocess
import shutil

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--emoji-font', type=pathlib.Path, required=True)
parser.add_argument('--cjk-font', type=pathlib.Path, required=True)
args = parser.parse_args()
out = pathlib.Path(__file__).resolve().parents[2] / 'apps/esp32/lvgl_port'
for font, size, ranges, name, extra in [
    (args.emoji_font, 16, '0x2600-0x27BF,0x1F300-0x1FAFF', 'operit_font_emoji_16',
     ['--lv-fallback', 'lv_font_montserrat_14']),
    (args.cjk_font, 28, '0x30-0x39', 'operit_font_digits_28', []),
]:
    subprocess.run([shutil.which('npx') or 'npx', '--yes', 'lv_font_conv@1.5.3',
                    '--font', str(font.resolve()), '-r', ranges, '--size', str(size),
                    '--bpp', '2', '--format', 'lvgl', '--lv-include', 'lvgl.h',
                    '--lv-font-name', name, '--no-kerning', '--no-compress',
                    '--output', str(out / (name + '.c')), *extra], check=True)
