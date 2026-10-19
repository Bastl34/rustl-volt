# py -3 ui/make_font.py <rvgl_assets.zip> ui/font.png - the font of the game: RVGL's font (gfx/font.bmo, 512 x 512, CC BY-NC-SA 4.0, see LICENSE.RVGL-font.txt)
# only the glyph rows ('!'..'~', 32 per row in 16 x 32 cells) and the tick / cross in the bottom right corner - white, the brightness times the alpha as alpha
import io, sys, zipfile
from PIL import Image

source, out = sys.argv[1], sys.argv[2]
SIZE = 512
GLYPH_ROWS = 3
CELL_H = 32
# tick and cross (the 256 page: 231..255 x 244..255)
MARKS = (462, 488, 512, 512)

font = Image.open(io.BytesIO(zipfile.ZipFile(source).read("gfx/font.bmo"))).convert("RGBA")
assert font.size == (SIZE, SIZE), font.size

result = Image.new("RGBA", (SIZE, SIZE), (255, 255, 255, 0))
for box in [(0, 0, SIZE, GLYPH_ROWS * CELL_H), MARKS]:
    part = font.crop(box)
    pixels = [(255, 255, 255, a * max(r, g, b) // 255) for r, g, b, a in part.getdata()]
    white = Image.new("RGBA", part.size)
    white.putdata(pixels)
    result.paste(white, box[:2])

result.save(out, optimize=True)
print(out, result.size)
