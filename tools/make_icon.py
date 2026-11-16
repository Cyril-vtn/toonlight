# -*- coding: utf-8 -*-
"""Toonlight's icon, an original design: a night-violet tile, a sheet of animation paper lit from
behind like on a light table (with its three peg holes), and a smiling little star that leaves an
onion-skin trail. PNG files and a Windows .ico, standard library only. Drawn with supersampling."""
import math, os, struct, sys, zlib

out = sys.argv[1]
os.makedirs(out, exist_ok=True)

def clamp(v, lo=0.0, hi=1.0):
    return lo if v < lo else hi if v > hi else v

def mix(a, b, t):
    return tuple(a[i] + (b[i] - a[i]) * t for i in range(3))

def rrect(x, y, hw, hh, r):
    qx, qy = abs(x) - hw + r, abs(y) - hh + r
    return math.hypot(max(qx, 0), max(qy, 0)) + min(max(qx, qy), 0) - r

def rot(x, y, ang):
    c, s = math.cos(ang), math.sin(ang)
    return x * c - y * s, x * s + y * c

K1 = (0.809016994375, -0.587785252292)
K2 = (-K1[0], K1[1])

def star(x, y, r, rf):
    """Signed distance to a five-pointed star pointing up (image coordinates, y down)."""
    px, py = abs(x), -y
    d = px * K1[0] + py * K1[1]
    if d > 0:
        px, py = px - 2 * d * K1[0], py - 2 * d * K1[1]
    d = px * K2[0] + py * K2[1]
    if d > 0:
        px, py = px - 2 * d * K2[0], py - 2 * d * K2[1]
    px = abs(px)
    py -= r
    bx, by = rf * -K1[1], rf * K1[0] - 1
    h = clamp((px * bx + py * by) / (bx * bx + by * by), 0, r)
    dist = math.hypot(px - bx * h, py - by * h)
    return dist if py * bx - px * by > 0 else -dist

def shade(X, Y, aa):
    col = (0.0, 0.0, 0.0)
    alpha = 0.0

    def over(c, a):
        nonlocal col, alpha
        if a <= 0:
            return
        oa = a + alpha * (1 - a)
        col = tuple((c[i] * a + col[i] * alpha * (1 - a)) / oa for i in range(3))
        alpha = oa

    cov = lambda d, w=aa: clamp(0.5 - d / w)
    tile = rrect(X - 512, Y - 512, 448, 448, 210)
    t = clamp((X + Y - 150) / 1700)
    over(mix((34, 26, 84), (122, 58, 196), t), cov(tile))
    if alpha <= 0:
        return col, alpha
    inside = cov(tile)
    # the light under the paper
    g = math.hypot(X - 512, Y - 560) / 470
    over((255, 205, 110), 0.62 * inside * clamp(1 - g) ** 2)
    # the sheet of paper, a little turned
    sx, sy = rot(X - 512, Y - 565, 0.06)
    over((40, 20, 70), 0.30 * clamp(0.5 - (rrect(sx - 8, sy - 22, 290, 240, 44)) / 46))
    paper = rrect(sx, sy, 290, 240, 44)
    pc = cov(paper)
    if pc > 0:
        # brighter in the middle, lit from behind
        lit = clamp(1 - math.hypot(sx, sy) / 420)
        over(mix((255, 236, 196), (255, 252, 238), lit), pc)
        # peg holes at the top: a round hole between two slots, glowing
        hy = sy + 188  # 52 px under the top edge
        for d in (math.hypot(sx, hy) - 20, rrect(sx - 165, hy, 46, 15, 15), rrect(sx + 165, hy, 46, 15, 15)):
            over((255, 196, 96), pc * cov(d))
        # onion-skin trail: earlier stars, smaller and bluer, along an arc
        for gx, gy, r, a in [(-175, 95, 70, 0.22), (-95, 25, 86, 0.36)]:
            lx, ly = rot(sx - gx, sy - gy, -0.35)
            over((96, 140, 255), a * pc * cov(star(lx, ly, r, 0.5) - 10))
        # the star itself
        lx, ly = rot(sx - 70, sy + 10, 0.12)
        d = star(lx, ly, 132, 0.52) - 16
        over((112, 36, 70), pc * cov(d - 9))  # outline
        over((255, 128, 72), pc * cov(d + 3))
        # a lighter top
        over((255, 186, 104), 0.55 * pc * cov(d + 3) * clamp(-(ly + 10) / 90))
        if d < 20:
            # eyes, cheeks, smile
            for ex in (-30, 30):
                over((52, 26, 58), pc * cov(math.hypot((lx - ex) / 0.72, ly + 4) - 15))
                over((255, 255, 255), pc * cov(math.hypot(lx - ex - 4, ly + 12) - 5))
            for cx in (-58, 58):
                over((255, 150, 160), 0.75 * pc * cov(math.hypot(lx - cx, ly - 24) - 13))
            if ly > 14:
                over((52, 26, 58), pc * cov(abs(math.hypot(lx, ly - 8) - 26) - 5.5))
    # two sparkles of light
    for cx, cy, r in [(790, 250, 34), (735, 330, 18)]:
        dx, dy = abs(X - cx), abs(Y - cy)
        d = min(math.hypot(dx / 0.22, dy) - r, math.hypot(dx, dy / 0.22) - r)
        over((255, 244, 214), inside * cov(d))
    return col, alpha

def draw(n, ss=4):
    px = bytearray(n * n * 4)
    k = 1024.0 / (n * ss)
    aa = k * 1.2
    for y in range(n):
        for x in range(n):
            r = g = b = a = 0.0
            for sy in range(ss):
                for sx in range(ss):
                    c, al = shade((x * ss + sx + 0.5) * k, (y * ss + sy + 0.5) * k, aa)
                    r += c[0] * al
                    g += c[1] * al
                    b += c[2] * al
                    a += al
            i = (y * n + x) * 4
            if a > 0:
                px[i], px[i + 1], px[i + 2] = int(round(r / a)), int(round(g / a)), int(round(b / a))
            px[i + 3] = int(round(a / (ss * ss) * 255))
    return bytes(px)

def png(n, data):
    raw = b''.join(b'\x00' + data[y * n * 4:(y + 1) * n * 4] for y in range(n))
    def chunk(t, d):
        return struct.pack('>I', len(d)) + t + d + struct.pack('>I', zlib.crc32(t + d) & 0xffffffff)
    return b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', n, n, 8, 6, 0, 0, 0)) + chunk(b'IDAT', zlib.compress(raw, 9)) + chunk(b'IEND', b'')

pngs = {512: png(512, draw(512, 3))}
for n in (32, 64, 128, 256):
    pngs[n] = png(n, draw(n, 4))
for n, d in pngs.items():
    open(os.path.join(out, 'icon-%d.png' % n), 'wb').write(d)
open(os.path.join(out, 'icon.png'), 'wb').write(pngs[512])
sizes = [32, 64, 128, 256]
header = struct.pack('<HHH', 0, 1, len(sizes))
offset = 6 + 16 * len(sizes)
entries = b''
blob = b''
for n in sizes:
    d = pngs[n]
    entries += struct.pack('<BBBBHHII', n % 256, n % 256, 0, 0, 1, 32, len(d), offset + len(blob))
    blob += d
open(os.path.join(out, 'icon.ico'), 'wb').write(header + entries + blob)
print('ok')
