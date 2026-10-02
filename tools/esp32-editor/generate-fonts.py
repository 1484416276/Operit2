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
    (args.cjk_font, 14, '0x2013-0x2014,0x2018-0x201f,0x2026,0x3001-0x3002,0x3008-0x3009,0x300c-0x3011,0x3014-0x3015,0xff01,0xff0c,0xff0e,0xff1a,0xff1b,0xff1f', 'operit_font_punctuation_14',
     ['--lv-fallback', 'operit_font_google_color']),
    (args.emoji_font, 16, '0x2600-0x27BF,0x1F300-0x1FAFF', 'operit_font_emoji_16',
     ['--lv-fallback', 'lv_font_montserrat_14']),
    (args.cjk_font, 28, '0x30-0x39', 'operit_font_digits_28', []),
]:
    subprocess.run([shutil.which('npx') or 'npx', '--yes', 'lv_font_conv@1.5.3',
                    '--font', str(font.resolve()), '-r', ranges, '--size', str(size),
                    '--bpp', '2', '--format', 'lvgl', '--lv-include', 'lvgl.h',
                    '--lv-font-name', name, '--no-kerning', '--no-compress',
                    '--output', str(out / (name + '.c')), *extra], check=True)
    if name == 'operit_font_punctuation_14':
        generated = out / (name + '.c')
        notice = ('/* Noto Sans CJK SC Regular; supplementary punctuation.\n'
                  ' * Copyright 2014-2021 Adobe (http://www.adobe.com/). All rights reserved.\n'
                  ' * SIL Open Font License 1.1; see OFL.txt.\n */\n')
        generated.write_text(notice + generated.read_text(encoding='utf-8'), encoding='utf-8')
