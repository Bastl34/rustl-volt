# py -3 ui/make_loading.py ui/loading.png - the picture of the setup screens (own, no file of the game)
# abstract loading picture for Rustl-Volt: night race track ribbon over a perspective grid, speed streaks, sparks
import math, random, sys
from PIL import Image, ImageDraw, ImageFilter, ImageChops

out = sys.argv[1]
W, H = 1224, 800
S = 2
w, h = W * S, H * S
random.seed(7)

def lerp(a, b, t): return a + (b - a) * t
def mix(c1, c2, t): return tuple(int(lerp(a, b, t)) for a, b in zip(c1, c2))

# background gradient + glow
img = Image.new("RGB", (w, h))
px = ImageDraw.Draw(img)
top, bottom = (14, 20, 70), (2, 3, 12)
for y in range(h):
    px.line([(0, y), (w, y)], fill=mix(top, bottom, (y / h) ** 0.8))
glow = Image.new("RGB", (w, h))
gd = ImageDraw.Draw(glow)
gd.ellipse([w * 0.15, h * 0.35, w * 0.85, h * 1.2], fill=(20, 40, 150))
glow = glow.filter(ImageFilter.GaussianBlur(220))
img = ImageChops.add(img, glow)

# perspective grid floor
horizon = h * 0.52
vx = w * 0.5
grid = Image.new("RGB", (w, h))
gd = ImageDraw.Draw(grid)
for i in range(-24, 25):
    x_bottom = vx + i * w * 0.09
    gd.line([(vx, horizon), (x_bottom, h)], fill=(40, 70, 210), width=3)
for k in range(1, 18):
    t = (k / 18) ** 2.2
    y = lerp(horizon, h, t)
    gd.line([(0, y), (w, y)], fill=(40, 70, 210), width=3)
mask = Image.new("L", (w, h))
md = ImageDraw.Draw(mask)
for y in range(int(horizon), h):
    md.line([(0, y), (w, y)], fill=int(150 * ((y - horizon) / (h - horizon)) ** 1.3))
img.paste(ImageChops.add(img, grid), (0, 0), mask)

# the track: a sweeping ribbon from the bottom left over a bend into the distance
def track_point(t):
    x = lerp(-0.1, 1.1, t) * w + math.sin(t * math.pi * 1.6) * w * 0.12
    y = h * (1.05 - 0.48 * t) - math.sin(t * math.pi) * h * 0.06
    return x, y

def width_at(t): return lerp(h * 0.34, h * 0.03, t ** 0.85)

N = 600
pts = [track_point(i / N) for i in range(N + 1)]
def normal(i):
    a, b = pts[max(i - 1, 0)], pts[min(i + 1, N)]
    dx, dy = b[0] - a[0], b[1] - a[1]
    l = math.hypot(dx, dy) or 1
    return -dy / l, dx / l

track = Image.new("RGBA", (w, h))
td = ImageDraw.Draw(track)
for i in range(N):
    t0, t1 = i / N, (i + 1) / N
    (x0, y0), (x1, y1) = pts[i], pts[i + 1]
    n0, n1 = normal(i), normal(i + 1)
    hw0, hw1 = width_at(t0) / 2, width_at(t1) / 2
    shade = int(lerp(34, 14, t0))
    quad = [(x0 + n0[0] * hw0, y0 + n0[1] * hw0), (x1 + n1[0] * hw1, y1 + n1[1] * hw1), (x1 - n1[0] * hw1, y1 - n1[1] * hw1), (x0 - n0[0] * hw0, y0 - n0[1] * hw0)]
    td.polygon(quad, fill=(shade, shade + 4, shade + 22, 255))
    # kerbs: red / white blocks on both edges
    kerb = (255, 40, 50, 255) if (i // 12) % 2 == 0 else (235, 235, 245, 255)
    for side in (1, -1):
        k0, k1 = hw0 * 1.0, hw1 * 1.0
        j0, j1 = hw0 * 0.86, hw1 * 0.86
        td.polygon([(x0 + side * n0[0] * k0, y0 + side * n0[1] * k0), (x1 + side * n1[0] * k1, y1 + side * n1[1] * k1),
                    (x1 + side * n1[0] * j1, y1 + side * n1[1] * j1), (x0 + side * n0[0] * j0, y0 + side * n0[1] * j0)], fill=kerb)
    # dashed center line
    if (i // 9) % 2 == 0:
        c = max(2, width_at(t0) * 0.025)
        td.polygon([(x0 + n0[0] * c, y0 + n0[1] * c), (x1 + n1[0] * c, y1 + n1[1] * c), (x1 - n1[0] * c, y1 - n1[1] * c), (x0 - n0[0] * c, y0 - n0[1] * c)], fill=(255, 210, 60, 255))
# the track fades into the distance
fade = Image.new("L", (w, h), 255)
fd = ImageDraw.Draw(fade)
for y in range(h):
    fd.line([(0, y), (w, y)], fill=int(255 * min(1, max(0, (y - h * 0.5) / (h * 0.25)))))
alpha = ImageChops.multiply(track.split()[3], fade)
track.putalpha(alpha)
shadow = Image.new("RGBA", (w, h), (0, 0, 0, 0))
shadow.putalpha(alpha.filter(ImageFilter.GaussianBlur(30)).point(lambda a: a * 0.7))
img = img.convert("RGBA")
img.alpha_composite(shadow, (0, 24))
img.alpha_composite(track)

# glow layer: speed streaks, bolts, sparks
light = Image.new("RGB", (w, h))
ld = ImageDraw.Draw(light)
for _ in range(70):
    y = random.uniform(h * 0.06, h * 0.56)
    x = random.uniform(-w * 0.1, w)
    length = random.uniform(w * 0.05, w * 0.35)
    c = random.choice([(60, 220, 255), (80, 120, 255), (255, 80, 200)])
    ld.line([(x, y), (x + length, y - length * 0.08)], fill=c, width=random.choice([2, 3, 4]))
def bolt(x, y, length, steps):
    pts = [(x, y)]
    for k in range(steps):
        x += random.uniform(-length * 0.25, length * 0.25)
        y += length / steps
        pts.append((x, y))
    ld.line(pts, fill=(140, 230, 255), width=7)
    ld.line(pts, fill=(255, 255, 255), width=3)
bolt(w * 0.08, h * 0.05, h * 0.45, 7)
bolt(w * 0.88, h * 0.08, h * 0.38, 6)
for _ in range(160):
    x, y = random.uniform(0, w), random.uniform(0, h)
    r = random.uniform(2, 6)
    ld.ellipse([x - r, y - r, x + r, y + r], fill=random.choice([(120, 230, 255), (255, 210, 90), (255, 120, 220)]))
blurred = light.filter(ImageFilter.GaussianBlur(14))
img = ImageChops.add(img.convert("RGB"), blurred)
img = ImageChops.add(img, light.point(lambda v: int(v * 0.8)))

# vignette
vig = Image.new("L", (w, h), 0)
vd = ImageDraw.Draw(vig)
vd.ellipse([-w * 0.15, -h * 0.25, w * 1.15, h * 1.25], fill=255)
vig = vig.filter(ImageFilter.GaussianBlur(180))
img = Image.composite(img, Image.new("RGB", (w, h), (0, 0, 0)), vig)

img.resize((W, H), Image.LANCZOS).save(out, optimize=True)
print("written", out)
