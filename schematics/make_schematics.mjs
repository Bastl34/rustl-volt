// generates the format schematics as svg and renders them to png with headless Edge
// node make_schematics.mjs   (png: 3840x2160)

import { writeFileSync, existsSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const DIR = dirname(fileURLToPath(import.meta.url));
const EDGE = 'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe';
const TOTAL = 21;

// ---------- style ----------

const C = {
    bg: '#0a0e16', dots: '#182033', panel: '#111827', border: '#243049', text: '#e8edf7', dim: '#8d97ad', faint: '#4b5672',
    accent: '#7c8cff',
    count: '#6c7cff', center: '#6fd08c', radius: '#3fc9c0', bbox: '#38b2d8', u16: '#f2b94b',
    flags: '#ff6b8b', tex: '#b38cff', idx: '#ff8f4d', color: '#ff7ad9', uv: '#4fd8ff', normal: '#3fa86a',
    x: '#ff6b6b', y: '#6fd08c', z: '#6c9cff',
};
const SANS = `'Segoe UI', Arial, sans-serif`;
const MONO = `'Cascadia Mono', Consolas, monospace`;

const esc = (s) => String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');

function attrs(o)
{
    return Object.entries(o).filter(([, v]) => v !== undefined && v !== null).map(([k, v]) => `${k}="${v}"`).join(' ');
}

// text - o: size, fill, weight, anchor, mono, ls, op
function T(x, y, s, o = {})
{
    return `<text ${attrs({
        x, y, style: 'white-space:pre', 'font-family': o.mono ? MONO : SANS, 'font-size': o.size ?? 22, fill: o.fill ?? C.text,
        'font-weight': o.weight, 'text-anchor': o.anchor, 'letter-spacing': o.ls, opacity: o.op, 'font-style': o.italic ? 'italic' : undefined,
    })}>${esc(s)}</text>`;
}

// text in several colors - parts: [text, fill, weight?, mono?]
function TS(x, y, parts, o = {})
{
    const spans = parts.map(([s, fill, weight, mono]) =>
        `<tspan ${attrs({ fill: fill ?? C.text, 'font-weight': weight, 'font-family': mono ? MONO : undefined })}>${esc(s)}</tspan>`).join('');
    return `<text ${attrs({ x, y, 'font-family': o.mono ? MONO : SANS, 'font-size': o.size ?? 22, 'text-anchor': o.anchor, fill: C.text })}>${spans}</text>`;
}

function R(x, y, w, h, o = {})
{
    return `<rect ${attrs({
        x, y, width: w, height: h, rx: o.r ?? 10, fill: o.fill ?? 'none', 'fill-opacity': o.fop, stroke: o.stroke,
        'stroke-width': o.sw ?? (o.stroke ? 2 : undefined), 'stroke-dasharray': o.dash, opacity: o.op, filter: o.shadow ? 'url(#shadow)' : undefined,
    })}/>`;
}

function L(x1, y1, x2, y2, o = {})
{
    return `<line ${attrs({
        x1, y1, x2, y2, stroke: o.stroke ?? C.faint, 'stroke-width': o.sw ?? 2, 'stroke-dasharray': o.dash,
        'marker-end': o.arrow ? `url(#${o.arrow === true ? 'arrow' : o.arrow})` : undefined, 'stroke-linecap': 'round', opacity: o.op,
    })}/>`;
}

function P(d, o = {})
{
    return `<path ${attrs({
        d, fill: o.fill ?? 'none', stroke: o.stroke, 'stroke-width': o.sw ?? 2, 'stroke-dasharray': o.dash, 'fill-opacity': o.fop,
        'marker-end': o.arrow ? `url(#${o.arrow === true ? 'arrow' : o.arrow})` : undefined, 'stroke-linecap': 'round', 'stroke-linejoin': 'round',
    })}/>`;
}

const panel = (x, y, w, h, o = {}) => R(x, y, w, h, { fill: C.panel, stroke: o.stroke ?? C.border, sw: o.sw ?? 1.5, r: 18, shadow: true, ...o });

// small colored tag with mono text
function pill(x, y, s, color, o = {})
{
    const size = o.size ?? 20;
    const w = o.w ?? s.length * size * 0.6 + 24;
    const h = size + 16;
    return R(x, y - h / 2, w, h, { fill: color, fop: 0.16, stroke: color, sw: 1.5, r: h / 2 })
        + T(x + w / 2, y + size * 0.35, s, { mono: true, size, fill: color, anchor: 'middle', weight: 600 });
}

function chip(x, y, color, size = 16)
{
    return R(x, y - size + 3, size, size, { fill: color, r: 4 });
}

// one record as a byte bar - fields: { name, type, bytes, color, fill?, sub?, subLabels?, outside? }
function byteBar(x, y, h, ppb, fields, o = {})
{
    let out = '';
    let cx = x;
    let offset = o.start ?? 0;
    const gap = 4;
    out += T(x, y + h + 34, offset, { mono: true, size: 18, fill: C.dim, anchor: 'start' });

    for (const f of fields)
    {
        const w = f.bytes * ppb;
        const bx = cx + gap / 2;
        const bw = w - gap;
        out += R(bx, y, bw, h, { fill: f.fill ?? f.color, fop: f.fill ? 0.35 : 0.16, stroke: f.color, sw: 2, r: 10 });

        // byte ticks
        if (ppb >= 14)
        {
            for (let b = 1; b < f.bytes; b++)
            {
                out += L(cx + b * ppb, y + h - 10, cx + b * ppb, y + h - 4, { stroke: f.color, sw: 1.5, op: 0.5 });
            }
        }

        // sub cells (v0 v1 v2 v3)
        if (f.sub)
        {
            const sw = w / f.sub;
            for (let s = 1; s < f.sub; s++)
            {
                out += L(cx + s * sw, y + 8, cx + s * sw, y + h - 8, { stroke: f.color, sw: 1.5, dash: '4 5', op: 0.7 });
            }
            (f.subLabels ?? []).forEach((label, s) =>
            {
                out += T(cx + s * sw + sw / 2, y + h / 2 + 26, label, { mono: true, size: 17, fill: f.color, anchor: 'middle', op: 0.85 });
            });
        }

        const nameY = f.sub ? y + h / 2 - 4 : y + h / 2 - 2;
        if (f.outside)
        {
            out += L(cx + w / 2, y - 8, cx + w / 2, y - 34, { stroke: f.color, sw: 1.5 });
            out += T(cx + w / 2 + (f.anchor == 'end' ? 6 : f.anchor == 'start' ? -6 : 0), y - 44, f.name, { size: 21, weight: 600, fill: f.color, anchor: f.anchor ?? 'middle' });
            out += T(cx + w / 2, y + h / 2 + 7, f.type, { mono: true, size: 16, fill: f.color, anchor: 'middle' });
        }
        else
        {
            out += T(cx + w / 2, nameY, f.name, { size: o.nameSize ?? 24, weight: 600, fill: f.color, anchor: 'middle' });
            if (!f.sub) out += T(cx + w / 2, nameY + 28, f.type, { mono: true, size: 17, fill: C.dim, anchor: 'middle' });
            else out += T(cx + w / 2, y - 14, f.type, { mono: true, size: 17, fill: C.dim, anchor: 'middle' });
        }

        cx += w;
        offset += f.bytes;
        out += T(cx, y + h + 34, offset, { mono: true, size: 18, fill: C.dim, anchor: 'middle' });
    }

    return out;
}

function page(index, kicker, title, subtitle, body)
{
    return `<svg xmlns="http://www.w3.org/2000/svg" width="1920" height="1080" viewBox="0 0 1920 1080">
<defs>
  <pattern id="dots" width="32" height="32" patternUnits="userSpaceOnUse"><circle cx="2" cy="2" r="1.6" fill="${C.dots}"/></pattern>
  <radialGradient id="glow" cx="85%" cy="-10%" r="75%"><stop offset="0" stop-color="#22305a" stop-opacity="0.85"/><stop offset="1" stop-color="${C.bg}" stop-opacity="0"/></radialGradient>
  <radialGradient id="glow2" cx="0%" cy="110%" r="60%"><stop offset="0" stop-color="#1c3a3a" stop-opacity="0.6"/><stop offset="1" stop-color="${C.bg}" stop-opacity="0"/></radialGradient>
  <linearGradient id="rgb" x1="0" x2="1"><stop offset="0" stop-color="#ff6b8b"/><stop offset="0.35" stop-color="#f2b94b"/><stop offset="0.65" stop-color="#6fd08c"/><stop offset="1" stop-color="#6c7cff"/></linearGradient>
  <pattern id="checker" width="20" height="20" patternUnits="userSpaceOnUse"><rect width="20" height="20" fill="#2a3246"/><rect width="10" height="10" fill="#3a4460"/><rect x="10" y="10" width="10" height="10" fill="#3a4460"/></pattern>
  <filter id="shadow" x="-10%" y="-10%" width="120%" height="135%"><feDropShadow dx="0" dy="10" stdDeviation="14" flood-color="#000" flood-opacity="0.5"/></filter>
  <filter id="soft" x="-50%" y="-50%" width="200%" height="200%"><feGaussianBlur stdDeviation="8"/></filter>
  <marker id="arrow" viewBox="0 0 10 10" refX="8" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse"><path d="M0,0 L10,5 L0,10 z" fill="${C.dim}"/></marker>
  <marker id="arrowA" viewBox="0 0 10 10" refX="8" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse"><path d="M0,0 L10,5 L0,10 z" fill="${C.accent}"/></marker>
  <marker id="arrowX" viewBox="0 0 10 10" refX="8" refY="5" markerWidth="6" markerHeight="6" orient="auto"><path d="M0,0 L10,5 L0,10 z" fill="${C.x}"/></marker>
  <marker id="arrowY" viewBox="0 0 10 10" refX="8" refY="5" markerWidth="6" markerHeight="6" orient="auto"><path d="M0,0 L10,5 L0,10 z" fill="${C.y}"/></marker>
  <marker id="arrowZ" viewBox="0 0 10 10" refX="8" refY="5" markerWidth="6" markerHeight="6" orient="auto"><path d="M0,0 L10,5 L0,10 z" fill="${C.z}"/></marker>
  <marker id="arrowW" viewBox="0 0 10 10" refX="8" refY="5" markerWidth="6" markerHeight="6" orient="auto"><path d="M0,0 L10,5 L0,10 z" fill="${C.text}"/></marker>
</defs>
<rect width="1920" height="1080" fill="${C.bg}"/>
<rect width="1920" height="1080" fill="url(#dots)"/>
<rect width="1920" height="1080" fill="url(#glow)"/>
<rect width="1920" height="1080" fill="url(#glow2)"/>
${R(96, 74, 6, 92, { fill: C.accent, r: 3 })}
${T(126, 98, kicker, { size: 19, weight: 700, fill: C.accent, ls: 5 })}
${T(124, 154, title, { size: 52, weight: 700 })}
${T(126, 196, subtitle, { size: 24, fill: C.dim })}
${body}
${T(96, 1040, 'RE-VOLT  →  RUSTL', { size: 16, weight: 700, fill: C.faint, ls: 4 })}
${T(1824, 1040, `${String(index).padStart(2, '0')} / ${TOTAL}`, { size: 16, weight: 700, fill: C.faint, ls: 3, anchor: 'end', mono: true })}
</svg>`;
}

// ---------- 01 level folder ----------

function levelFolder()
{
    const cards = [
        { title: 'Graphics', color: C.center, rows: [
            ['.w', 'World mesh', 'the whole track · 1.1 MB', true],
            ['.prm', 'Models', '25 props: forklift, trolley, tins'],
            ['.bmp', 'Texture pages', 'market1a … market1j · 256×256'],
            ['.bmq', 'Low-res copies', '128×128 · ignored'],
            ['.rim', 'Mirror planes', 'the shiny supermarket floor'],
        ] },
        { title: 'Scene', color: C.tex, rows: [
            ['.inf', 'Level info', 'name, start, fog, music · text'],
            ['.fin', 'Instances', 'placed .prm models'],
            ['.fob', 'Game objects', 'pickups, barrels, stars'],
            ['.lit', 'Lights', 'level light sources'],
            ['.vis', 'Visiboxes', 'culling · not needed'],
        ] },
        { title: 'Physics', color: C.flags, rows: [
            ['.ncp', 'Collision', 'planes + surface material'],
            ['.fld', 'Force fields', 'wind, conveyors · other levels'],
            ['.hul', 'Car hulls', 'convex hulls · in cars/'],
        ] },
        { title: 'Race & AI', color: C.u16, rows: [
            ['.fan', 'AI nodes', 'racing line for the bots'],
            ['.pan', 'Position nodes', 'race position, lap progress'],
            ['.taz', 'Track zones', 'checkpoint order, respawn'],
            ['.tri', 'Triggers', 'piano, camera, reposition'],
            ['.cam', 'Camera nodes', 'replays, fly-throughs'],
        ] },
    ];

    let body = '';
    const w = 408, gap = 32, y = 262, h = 720;
    cards.forEach((card, i) =>
    {
        const x = 96 + i * (w + gap);
        body += panel(x, y, w, h);
        body += R(x, y, w, 8, { fill: card.color, r: 4 });
        body += T(x + 32, y + 64, card.title, { size: 30, weight: 700, fill: card.color });
        body += L(x + 32, y + 90, x + w - 32, y + 90, { stroke: C.border, sw: 1.5 });

        card.rows.forEach(([ext, name, desc, focus], r) =>
        {
            const ry = y + 150 + r * 118;
            if (focus)
            {
                body += R(x + 16, ry - 44, w - 32, 104, { fill: card.color, fop: 0.08, stroke: card.color, sw: 1.5, r: 14 });
                body += T(x + w - 34, ry - 14, 'TODAY', { size: 14, weight: 700, fill: card.color, ls: 3, anchor: 'end' });
            }
            body += pill(x + 32, ry, ext, card.color, { w: 92, size: 21 });
            body += T(x + 144, ry + 8, name, { size: 25, weight: 600 });
            body += T(x + 144, ry + 40, desc, { size: 18, fill: C.dim });
        });
    });

    return page(1, 'RE-VOLT FILE FORMATS', 'Inside a level folder', 'levels/market1/ · one folder per track, one file type per job', body);
}

// ---------- 02 the .w file ----------

function worldFile()
{
    let body = '';
    const x = 96, w = 760;
    let y = 262;

    const block = (h, color, left, right, o = {}) =>
    {
        let s = R(x, y, w, h, { fill: color, fop: o.big ? 0.12 : 0.18, stroke: color, sw: 2, r: 12, dash: o.dash });
        s += T(x + 28, o.top ? y + 52 : y + h / 2 + 9, left, { size: o.big ? 30 : 23, weight: o.big ? 700 : 600, fill: color, mono: !o.big });
        if (right) s += T(x + w - 28, y + h / 2 + 8, right, { size: 21, fill: C.dim, anchor: 'end', mono: true });
        y += h + 10;
        return s;
    };

    body += block(50, C.count, 'i32 mesh_count', '= 1452');
    const meshY = y;
    body += block(270, C.center, 'Mesh × 1452', null, { big: true, top: true });
    // the inside of a mesh
    const inner = [['bounds', C.radius, '40 B'], ['counts', C.u16, '4 B'], ['polygons', C.flags, '60 B each'], ['vertices', C.normal, '24 B each']];
    inner.forEach(([name, color, size], i) =>
    {
        const iy = meshY + 92 + i * 42;
        body += R(x + 28, iy, w - 56, 34, { fill: color, fop: 0.14, stroke: color, sw: 1.2, r: 8 });
        body += T(x + 46, iy + 24, name, { size: 19, fill: color, weight: 600 });
        body += T(x + w - 46, iy + 24, size, { size: 17, fill: C.dim, anchor: 'end', mono: true });
    });
    body += block(50, C.count, 'i32 big_cube_count', '= 57');
    body += block(78, C.radius, 'BigCube × 57', 'center, radius, mesh ids', { big: true });
    body += block(50, C.count, 'i32 tex_anim_count', '= 1');
    body += block(64, C.tex, 'TexAnim × 1', '31 frames', { big: true });
    body += block(64, C.u16, 'u32 env_colors × 1837', 'no count stored!', { dash: '8 6' });

    // right side - where the bytes go
    const rx = 940, rw = 884;
    body += panel(rx, 262, rw, 330);
    body += T(rx + 36, 318, 'Where the bytes go', { size: 28, weight: 700 });
    body += T(rx + 36, 352, 'market1.w · 1,174,104 bytes', { size: 20, fill: C.dim, mono: true });
    const parts = [['Meshes', 1158552, C.center], ['Big cubes', 6952, C.radius], ['Texture anim', 1248, C.tex], ['Env colors', 7348, C.u16]];
    let bx = rx + 36;
    const bw = rw - 72;
    parts.forEach(([, bytes, color]) =>
    {
        const pw = Math.max(6, (bytes / 1174104) * bw);
        body += R(bx, 382, pw - 2, 46, { fill: color, fop: 0.8, r: 6 });
        bx += pw;
    });
    parts.forEach(([name, bytes, color], i) =>
    {
        const lx = rx + 36 + (i % 2) * 420, ly = 482 + Math.floor(i / 2) * 50;
        body += chip(lx, ly, color, 18);
        body += T(lx + 30, ly, name, { size: 22, weight: 600 });
        body += T(lx + 380, ly, `${(bytes / 1174104 * 100).toFixed(1)} %`, { size: 20, fill: C.dim, anchor: 'end', mono: true });
    });

    const notes = [
        [C.center, 'Mesh', 'a chunk of the track with its own bounding sphere'],
        [C.radius, 'BigCube', 'groups meshes for culling → one glTF node each'],
        [C.tex, 'TexAnim', 'frames of (texture page, delay, 4 UVs)'],
        [C.u16, 'Env colors', 'count = polygons with the ENV flag (2048)'],
    ];
    notes.forEach(([color, name, text], i) =>
    {
        const ny = 640 + i * 86;
        body += panel(rx, ny, rw, 70, { stroke: C.border });
        body += R(rx, ny, 6, 70, { fill: color, r: 3 });
        body += T(rx + 32, ny + 44, name, { size: 23, weight: 700, fill: color });
        body += T(rx + 210, ny + 44, text, { size: 22, fill: C.text });
    });

    return page(2, 'RE-VOLT FILE FORMATS', 'The .w world file', 'Four sections, read straight through · little endian · no file header', body);
}

// ---------- 03 mesh record ----------

function meshRecord()
{
    let body = '';
    const x = 96, ppb = 1728 / 44, y = 330, h = 110;
    body += byteBar(x, y, h, ppb, [
        { name: 'center', type: 'f32 × 3', bytes: 12, color: C.center },
        { name: 'radius', type: 'f32', bytes: 4, color: C.radius },
        { name: 'bounding box', type: 'f32 × 6', bytes: 24, color: C.bbox, sub: 6, subLabels: ['xlo', 'xhi', 'ylo', 'yhi', 'zlo', 'zhi'] },
        { name: 'polys', type: 'u16', bytes: 2, color: C.u16, outside: true },
        { name: 'verts', type: 'u16', bytes: 2, color: C.u16, outside: true },
    ]);
    body += T(x, y - 60, 'Mesh header · 44 bytes', { size: 22, weight: 600, fill: C.dim });

    // arrays
    const ay = 590;
    const cards = [
        [96, 'Polygon × polys', '60 bytes each', C.flags, ['flags · texture page', '4 vertex indices', '4 colors · 4 UVs']],
        [700, 'Vertex × verts', '24 bytes each', C.normal, ['position  f32 × 3', 'normal    f32 × 3']],
    ];
    cards.forEach(([cx, title, size, color, lines], i) =>
    {
        const cw = 560;
        body += panel(cx, ay, cw, 230, { stroke: color, sw: 1.5 });
        body += T(cx + 32, ay + 56, title, { size: 28, weight: 700, fill: color });
        body += T(cx + cw - 32, ay + 56, size, { size: 19, fill: C.dim, anchor: 'end', mono: true });
        lines.forEach((line, l) => { body += T(cx + 32, ay + 110 + l * 40, line, { size: 22, mono: i == 1, fill: C.text }); });
    });
    // arrows from the counts
    const px = x + 41 * ppb, vx = x + 43 * ppb;
    body += P(`M ${px} ${y + h + 50} L ${px} ${ay - 70} L 376 ${ay - 70} L 376 ${ay - 10}`, { stroke: C.u16, sw: 2, dash: '6 6', arrow: true });
    body += P(`M ${vx} ${y + h + 50} L ${vx} ${ay - 40} L 980 ${ay - 40} L 980 ${ay - 10}`, { stroke: C.u16, sw: 2, dash: '6 6', arrow: true });

    // gotchas + example
    const gx = 1300, gw = 524;
    body += panel(gx, ay, gw, 230);
    body += T(gx + 32, ay + 56, 'Watch out', { size: 26, weight: 700, fill: C.u16 });
    body += T(gx + 32, ay + 102, 'The box is min/max per axis:', { size: 21 });
    body += T(gx + 32, ay + 136, 'xlo xhi ylo yhi zlo zhi', { size: 21, mono: true, fill: C.bbox });
    body += T(gx + 32, ay + 178, '.prm / .m files (cars, props) use the', { size: 21 });
    body += T(gx + 32, ay + 208, 'same body, without the 40-byte bounds.', { size: 21 });

    body += panel(96, 860, 1728, 120);
    body += T(128, 908, 'market1 · mesh 0', { size: 22, weight: 700, fill: C.accent });
    body += TS(128, 950, [
        ['center ', C.dim], ['(−3125, −1375, −1000)', C.center, 600, true], ['   radius ', C.dim], ['395', C.radius, 600, true],
        ['   polygons ', C.dim], ['3', C.u16, 600, true], ['   vertices ', C.dim], ['8', C.u16, 600, true],
        ['   →  44 + 3 × 60 + 8 × 24 = ', C.dim], ['416 bytes', C.text, 700, true],
    ], { size: 23 });

    return page(3, 'RE-VOLT FILE FORMATS', 'One mesh', 'A 44-byte header, then its polygon array and its vertex array', body);
}

// ---------- 04 polygon + vertex ----------

function polygonRecord()
{
    let body = '';
    const x = 96, ppb = 1728 / 60, y = 356, h = 120;
    body += byteBar(x, y, h, ppb, [
        { name: 'flags', type: 'u16', bytes: 2, color: C.flags, outside: true, anchor: 'end' },
        { name: 'texture', type: 'i16', bytes: 2, color: C.tex, outside: true, anchor: 'start' },
        { name: 'indices', type: 'u16 × 4', bytes: 8, color: C.idx, sub: 4, subLabels: ['v0', 'v1', 'v2', 'v3'] },
        { name: 'colors', type: 'u32 × 4  ·  BGRA', bytes: 16, color: C.color, fill: 'url(#rgb)', sub: 4, subLabels: ['c0', 'c1', 'c2', 'c3'] },
        { name: 'uvs', type: '(f32, f32) × 4', bytes: 32, color: C.uv, sub: 4, subLabels: ['uv0', 'uv1', 'uv2', 'uv3'] },
    ], { nameSize: 24 });
    body += T(x, 262, 'Polygon · 60 bytes', { size: 22, weight: 600, fill: C.dim });

    const notes = [
        [C.flags, 'flags', ['bitfield: quad, double-', 'sided, translucent, env …']],
        [C.tex, 'texture', ['page 0–9 → market1a–j', '−1 = no texture']],
        [C.idx, 'indices', ['into this mesh’s vertices', 'triangles ignore v3']],
        [C.color, 'colors', ['pre-baked lighting per', 'corner · A only if translucent']],
        [C.uv, 'uvs', ['per corner · 0 … 1', 'origin top-left']],
    ];
    const nw = (1728 - 4 * 24) / 5;
    notes.forEach(([color, name, lines], i) =>
    {
        const nx = 96 + i * (nw + 24), ny = 556;
        body += panel(nx, ny, nw, 150);
        body += R(nx, ny, nw, 6, { fill: color, r: 3 });
        body += T(nx + 24, ny + 48, name, { size: 23, weight: 700, fill: color });
        lines.forEach((line, l) => { body += T(nx + 24, ny + 88 + l * 32, line, { size: 19, fill: C.text }); });
    });

    // vertex
    const vy = 800;
    body += T(x, vy - 32, 'Vertex · 24 bytes', { size: 22, weight: 600, fill: C.dim });
    body += byteBar(x, vy, 96, ppb, [
        { name: 'position', type: 'f32 × 3', bytes: 12, color: C.center },
        { name: 'normal', type: 'f32 × 3', bytes: 12, color: C.normal },
    ]);

    // corner illustration
    const qx = 1010, qy = 740;
    body += panel(qx - 30, qy, 844, 240);
    const pts = [[qx + 70, qy + 60], [qx + 330, qy + 50], [qx + 350, qy + 190], [qx + 50, qy + 200]];
    const cols = ['#ff6b8b', '#f2b94b', '#6fd08c', '#6c7cff'];
    body += P(`M ${pts.map((p) => p.join(' ')).join(' L ')} Z`, { fill: 'url(#rgb)', fop: 0.25, stroke: C.text, sw: 2 });
    pts.forEach(([px, py], i) =>
    {
        body += `<circle cx="${px}" cy="${py}" r="13" fill="${cols[i]}"/>`;
        body += T(px, py + 6, i, { size: 16, weight: 700, fill: '#0a0e16', anchor: 'middle' });
    });
    body += T(qx + 420, qy + 66, 'A corner = ', { size: 22, fill: C.dim });
    body += TS(qx + 420, qy + 104, [['index', C.idx, 600], [' + ', C.dim], ['color', C.color, 600], [' + ', C.dim], ['uv', C.uv, 600]], { size: 24 });
    body += T(qx + 420, qy + 152, 'Colors and UVs live on the polygon,', { size: 19, fill: C.text });
    body += T(qx + 420, qy + 180, 'so every corner becomes its own', { size: 19, fill: C.text });
    body += T(qx + 420, qy + 208, 'glTF vertex.', { size: 19, fill: C.text });

    return page(4, 'RE-VOLT FILE FORMATS', 'Polygon and vertex', 'Quads and triangles share one 60-byte layout', body);
}

// ---------- 05 hex dump ----------

function hexDump()
{
    const hex = 'ac050000005043c500dcabc400007ac473a4c54300c05ac500e02bc5007cbbc4ff3b9cc400007ac400007ac4030008000100050000000100020003008e8e8e008e8e8e008e8e8e008e8e8e000000003f0000003f0000803e0000003f0000803e0000803e0000003f0000803e';
    const bytes = hex.match(/../g);
    const fields = [[0, 4, C.count], [4, 16, C.center], [16, 20, C.radius], [20, 44, C.bbox], [44, 48, C.u16], [48, 50, C.flags], [50, 52, C.tex], [52, 60, C.idx], [60, 76, C.color], [76, 108, C.uv]];
    const colorOf = (i) => fields.find(([a, b]) => i >= a && i < b)[2];

    let body = '';
    const x0 = 96, y0 = 280, cell = 44, rowH = 60;
    body += panel(x0 - 24, y0 - 50, 920, 520);
    for (let c = 0; c < 16; c++)
    {
        body += T(x0 + 130 + c * cell + (c >= 8 ? 14 : 0) + 18, y0 - 12, c.toString(16).toUpperCase(), { mono: true, size: 16, fill: C.faint, anchor: 'middle' });
    }
    bytes.forEach((b, i) =>
    {
        const row = Math.floor(i / 16), col = i % 16;
        const bx = x0 + 130 + col * cell + (col >= 8 ? 14 : 0), by = y0 + 14 + row * rowH;
        if (col == 0) body += T(x0 + 6, by + 27, (row * 16).toString(16).padStart(4, '0'), { mono: true, size: 20, fill: C.faint });
        const color = colorOf(i);
        body += R(bx, by, cell - 6, 40, { fill: color, fop: 0.16, stroke: color, sw: 1, r: 6 });
        body += T(bx + (cell - 6) / 2, by + 27, b, { mono: true, size: 20, fill: color, anchor: 'middle', weight: 600 });
    });
    body += T(x0 + 6, y0 + 440, 'bytes 0x00 – 0x6B · mesh_count, mesh 0 header, polygon 0', { size: 18, fill: C.dim });

    const rows = [
        ['FILE'],
        [C.count, 'mesh_count', 'ac 05 00 00', '1452'],
        ['MESH 0'],
        [C.center, 'center', '3 × f32', '(−3125, −1374.9, −1000)'],
        [C.radius, 'radius', '73 a4 c5 43', '395.3'],
        [C.bbox, 'bounding box', '6 × f32', 'x −3500…−2750 · z −1000'],
        [C.u16, 'polys / verts', '03 00 08 00', '3 / 8'],
        ['POLYGON 0'],
        [C.flags, 'flags', '01 00', 'QUAD'],
        [C.tex, 'texture', '05 00', '5 → market1f.bmp'],
        [C.idx, 'indices', '0 1 2 3', 'one quad'],
        [C.color, 'colors', '8e 8e 8e 00', 'gray 55 % · A 0'],
        [C.uv, 'uvs', '8 × f32', '(.5 .5) (.25 .5) (.25 .25) (.5 .25)'],
    ];
    const rx = 1040;
    let ry = 268;
    rows.forEach((r) =>
    {
        if (r.length == 1)
        {
            ry += 14;
            body += T(rx, ry, r[0], { size: 15, weight: 700, fill: C.faint, ls: 4 });
            ry += 30;
            return;
        }
        const [color, name, raw, value] = r;
        body += chip(rx, ry, color, 16);
        body += T(rx + 28, ry, name, { size: 21, weight: 600, fill: color });
        body += T(rx + 220, ry, raw, { size: 18, mono: true, fill: C.dim });
        body += T(rx + 390, ry, value, { size: 20, mono: true, fill: C.text });
        ry += 44;
    });

    body += panel(72, 800, 920, 170);
    body += T(104, 848, 'Little endian', { size: 26, weight: 700, fill: C.count });
    body += TS(104, 894, [['ac 05 00 00', C.count, 600, true], ['  →  read backwards  →  ', C.dim], ['0x000005ac', C.count, 600, true], [' = 1452', C.text]], { size: 23 });
    body += TS(104, 938, [['00 00 7a c4', C.center, 600, true], ['  →  ', C.dim], ['0xc47a0000', C.center, 600, true], [' = −1000.0 (IEEE 754 float)', C.text]], { size: 23 });

    return page(5, 'RE-VOLT FILE FORMATS', 'Reading the first bytes', 'market1.w in hex · every color is one field', body);
}

// ---------- 06 flags ----------

function polygonFlags()
{
    const used = {
        0: ['QUAD', C.idx], 1: ['DOUBLE', C.center], 2: ['TRANSL', C.uv], 7: ['MIRROR', C.bbox], 8: ['ADD', C.color],
        9: ['TEXANIM', C.tex], 10: ['NOENV', C.faint], 11: ['ENV', C.u16], 13: ['SKIP', C.flags],
    };
    let body = '';
    const cw = 100, gap = 8, y = 252, h = 104;
    for (let bit = 15; bit >= 0; bit--)
    {
        const x = 96 + (15 - bit) * (cw + gap);
        const u = used[bit];
        const color = u ? u[1] : C.border;
        body += R(x, y, cw, h, { fill: u ? color : C.panel, fop: u ? 0.16 : 1, stroke: color, sw: u ? 2 : 1.5, r: 10 });
        body += T(x + cw / 2, y + 34, `bit ${bit}`, { size: 16, mono: true, fill: u ? color : C.faint, anchor: 'middle' });
        body += T(x + cw / 2, y + 66, u ? u[0] : '—', { size: u && u[0].length > 6 ? 17 : 20, weight: 700, fill: u ? color : C.faint, anchor: 'middle' });
        body += T(x + cw / 2, y + 94, 1 << bit, { size: 15, mono: true, fill: C.dim, anchor: 'middle' });
    }
    body += T(96, y + h + 34, 'u16 flags · bit 15 left, bit 0 right', { size: 18, fill: C.dim });

    const rows = [
        ['QUAD', 1, 8842, C.idx, '4 corners → 2 triangles (else 3)'],
        ['DOUBLE', 2, 388, C.center, 'material doubleSided'],
        ['TRANSLUCENT', 4, 78, C.uv, 'alphaMode BLEND · alpha from vertex color'],
        ['MIRROR', 128, 1817, C.bbox, 'ignored for now · planar mirrors later'],
        ['ADDITIVE', 256, 78, C.color, 'BLEND + extras.revolt_blend = "additive"'],
        ['TEXANIM', 512, 16, C.tex, 'texture = animation index → frame 0'],
        ['NOENV', 1024, 0, C.faint, '—'],
        ['ENV', 2048, 1837, C.u16, 'own material · extras.revolt_env · COLOR_1'],
        ['SKIP', 8192, 0, C.flags, 'dropped'],
    ];
    const ty = 446, rh = 56;
    body += panel(96, ty - 30, 1728, rows.length * rh + 66);
    body += T(128, ty + 6, 'FLAG', { size: 15, weight: 700, fill: C.faint, ls: 3 });
    body += T(440, ty + 6, 'POLYGONS IN MARKET1 (OF 9842)', { size: 15, weight: 700, fill: C.faint, ls: 3 });
    body += T(1110, ty + 6, 'CONVERTER', { size: 15, weight: 700, fill: C.faint, ls: 3 });
    rows.forEach(([name, value, count, color, action], i) =>
    {
        const ry = ty + 56 + i * rh;
        body += chip(128, ry, color, 16);
        body += T(156, ry, name, { size: 22, weight: 700, fill: color });
        body += T(400, ry, value, { size: 19, mono: true, fill: C.dim, anchor: 'end' });
        const bw = 520 * count / 9842;
        body += R(440, ry - 18, 520, 22, { fill: C.border, fop: 0.5, r: 6 });
        if (count > 0) body += R(440, ry - 18, Math.max(4, bw), 22, { fill: color, fop: 0.85, r: 6 });
        body += T(980, ry, count.toLocaleString('en-US'), { size: 19, mono: true, fill: C.text });
        body += T(1110, ry, action, { size: 20, fill: C.text });
    });

    return page(6, 'RE-VOLT FILE FORMATS', 'Polygon flags', 'One u16 per polygon decides how it is drawn', body);
}

// ---------- 07 coordinates ----------

function coordinates()
{
    let body = '';

    const gizmo = (cx, cy, title, sub, yUp, zToViewer) =>
    {
        let s = '';
        const len = 150;
        s += L(cx, cy, cx + len, cy, { stroke: C.x, sw: 5, arrow: 'arrowX' });
        s += T(cx + len + 18, cy + 9, 'X', { size: 28, weight: 700, fill: C.x });
        const yy = yUp ? cy - len : cy + len;
        s += L(cx, cy, cx, yy, { stroke: C.y, sw: 5, arrow: 'arrowY' });
        s += T(cx - 14, yUp ? yy - 16 : yy + 40, 'Y', { size: 28, weight: 700, fill: C.y, anchor: 'middle' });
        const zx = zToViewer ? cx - 100 : cx + 90, zy = zToViewer ? cy + 80 : cy - 72;
        s += L(cx, cy, zx, zy, { stroke: C.z, sw: 5, arrow: 'arrowZ', dash: zToViewer ? undefined : '10 8' });
        s += T(zx + (zToViewer ? -26 : 22), zy + (zToViewer ? 28 : -6), 'Z', { size: 28, weight: 700, fill: C.z });
        s += `<circle cx="${cx}" cy="${cy}" r="8" fill="${C.text}"/>`;
        return s;
    };

    // two systems
    body += panel(96, 252, 540, 420);
    body += T(128, 304, 'Re-Volt', { size: 32, weight: 700 });
    body += T(128, 340, 'Y down · Z into the screen', { size: 20, fill: C.dim });
    body += gizmo(300, 470, '', '', false, false);

    body += panel(1284, 252, 540, 420);
    body += T(1316, 304, 'glTF / rustl', { size: 32, weight: 700 });
    body += T(1316, 340, 'Y up · Z toward the viewer', { size: 20, fill: C.dim });
    body += gizmo(1500, 540, '', '', true, true);

    body += L(680, 440, 1240, 440, { stroke: C.accent, sw: 3, arrow: 'arrowA' });
    body += T(960, 400, '(x, y, z)  →  (x, −y, −z) × 0.01', { size: 32, weight: 700, mono: true, anchor: 'middle' });
    body += T(960, 496, '180° turn around X · no mirroring', { size: 22, fill: C.dim, anchor: 'middle' });
    body += T(960, 532, 'units → meters', { size: 22, fill: C.dim, anchor: 'middle' });

    // winding
    const wy = 710;
    body += panel(96, wy, 1100, 280);
    body += T(128, wy + 50, 'Winding', { size: 28, weight: 700 });
    body += T(128, wy + 84, 'front faces: clockwise in Re-Volt (Direct3D), counter-clockwise in glTF', { size: 19, fill: C.dim });

    const quad = (ox, oy, label) =>
    {
        const p = [[ox, oy], [ox + 150, oy], [ox + 150, oy + 130], [ox, oy + 130]];
        let s = P(`M ${p.map((q) => q.join(' ')).join(' L ')} Z`, { fill: C.idx, fop: 0.12, stroke: C.idx, sw: 2 });
        p.forEach(([qx, qy], i) =>
        {
            s += `<circle cx="${qx}" cy="${qy}" r="14" fill="${C.idx}"/>` + T(qx, qy + 6, i, { size: 16, weight: 700, fill: C.bg, anchor: 'middle' });
        });
        s += T(ox + 75, oy + 164, label, { size: 18, fill: C.dim, anchor: 'middle', mono: true });
        return s;
    };
    body += quad(170, wy + 104, '0 → 1 → 2 → 3');
    body += P(`M 214 ${wy + 144} A 40 40 0 1 1 214 ${wy + 194}`, { stroke: C.text, sw: 2.5, arrow: 'arrowW' });
    
    body += L(400, wy + 169, 520, wy + 169, { stroke: C.accent, sw: 3, arrow: 'arrowA' });

    // two triangles
    const tx = 580, ty = wy + 104;
    body += P(`M ${tx} ${ty} L ${tx + 150} ${ty + 130} L ${tx + 150} ${ty} Z`, { fill: C.uv, fop: 0.14, stroke: C.uv, sw: 2 });
    body += P(`M ${tx} ${ty} L ${tx} ${ty + 130} L ${tx + 150} ${ty + 130} Z`, { fill: C.center, fop: 0.14, stroke: C.center, sw: 2 });
    [[tx, ty, 0], [tx + 150, ty, 1], [tx + 150, ty + 130, 2], [tx, ty + 130, 3]].forEach(([qx, qy, i]) =>
    {
        body += `<circle cx="${qx}" cy="${qy}" r="14" fill="${C.text}"/>` + T(qx, qy + 6, i, { size: 16, weight: 700, fill: C.bg, anchor: 'middle' });
    });
    body += T(820, wy + 152, '(0, 2, 1)', { size: 24, mono: true, weight: 700, fill: C.uv });
    body += T(820, wy + 196, '(0, 3, 2)', { size: 24, mono: true, weight: 700, fill: C.center });
    body += T(820, wy + 240, 'swap the last two', { size: 18, fill: C.dim });

    // scale
    const sx = 1228, sw = 596;
    body += panel(sx, wy, sw, 280);
    body += T(sx + 32, wy + 50, 'Scale', { size: 28, weight: 700 });
    body += T(sx + 32, wy + 84, '1 unit ≈ 1 cm · rustl works in meters', { size: 19, fill: C.dim });
    const scale = [['RC car wheel radius', '10', '0.10 m'], ['RC car wheelbase', '84', '0.84 m'], ['Supermarket', '20500', '205 m']];
    scale.forEach(([name, units, meters], i) =>
    {
        const ry = wy + 140 + i * 46;
        body += T(sx + 32, ry, name, { size: 21 });
        body += T(sx + 410, ry, units, { size: 21, mono: true, fill: C.dim, anchor: 'end' });
        body += T(sx + 430, ry, '→', { size: 21, fill: C.faint });
        body += T(sx + sw - 32, ry, meters, { size: 21, mono: true, weight: 700, fill: C.center, anchor: 'end' });
    });

    return page(7, 'RE-VOLT FILE FORMATS', 'Coordinates, winding, scale', 'Three small things that break everything when they are wrong', body);
}

// ---------- 08 textures ----------

function textures()
{
    let body = '';

    // page index -> file
    const y = 262;
    body += T(96, y + 10, 'polygon.texture  →  file', { size: 22, weight: 600, fill: C.dim });
    const cw = 150, gap = 14;
    for (let i = 0; i < 10; i++)
    {
        const x = 96 + i * (cw + gap);
        body += R(x, y + 36, cw, 120, { fill: C.tex, fop: 0.12, stroke: C.tex, sw: 1.5, r: 12 });
        body += T(x + cw / 2, y + 86, i, { size: 34, weight: 700, fill: C.tex, anchor: 'middle', mono: true });
        body += T(x + cw / 2, y + 128, `market1${String.fromCharCode(97 + i)}`, { size: 19, mono: true, fill: C.text, anchor: 'middle' });
    }
    body += R(96 + 10 * (cw + gap), y + 36, 1824 - (96 + 10 * (cw + gap)), 120, { fill: C.panel, stroke: C.faint, sw: 1.5, r: 12, dash: '6 6' });
    body += T(96 + 10 * (cw + gap) + 54, y + 86, '−1', { size: 34, weight: 700, fill: C.dim, anchor: 'middle', mono: true });
    body += T(96 + 10 * (cw + gap) + 54, y + 128, 'none', { size: 19, mono: true, fill: C.dim, anchor: 'middle' });

    // pixel art: a little bolt on black
    const art = [
        '............',
        '.......##...',
        '......##....',
        '.....##.....',
        '....#####...',
        '...#####....',
        '.....##.....',
        '....##......',
        '...##.......',
        '..##........',
        '............',
        '............',
    ];
    const sprite = (ox, oy, keyed) =>
    {
        let s = keyed ? R(ox, oy, 216, 216, { fill: 'url(#checker)', r: 6 }) : R(ox, oy, 216, 216, { fill: '#000', r: 6 });
        art.forEach((row, ry) => [...row].forEach((c, rx) =>
        {
            if (c == '#') s += `<rect x="${ox + rx * 18}" y="${oy + ry * 18}" width="18" height="18" fill="${(rx + ry) % 3 ? '#ffd76e' : '#ff9e4d'}"/>`;
        }));
        return s + R(ox, oy, 216, 216, { stroke: C.border, sw: 2, r: 6 });
    };

    const sy = 480;
    const steps = [
        [96, 'BMP', ['256 × 256 · 24-bit', 'rows bottom-up', '54-byte header'], C.u16],
        [560, 'Color key', ['pure black #000000', '→ alpha 0'], C.flags],
        [1024, 'PNG · RGBA', ['embedded in the .glb', 'as an image buffer'], C.center],
        [1488, 'Material', ['MASK if any pixel', 'was keyed, else OPAQUE'], C.tex],
    ];
    steps.forEach(([x, title, lines, color], i) =>
    {
        body += panel(x, sy, 336, 470);
        body += R(x, sy, 336, 6, { fill: color, r: 3 });
        body += T(x + 32, sy + 56, title, { size: 28, weight: 700, fill: color });
        if (i < 3) body += L(x + 350, sy + 235, x + 450, sy + 235, { stroke: C.accent, sw: 3, arrow: 'arrowA' });
        lines.forEach((line, l) => { body += T(x + 32, sy + 370 + l * 32, line, { size: 20, fill: C.text }); });
    });
    body += sprite(96 + 60, sy + 92, false);
    body += sprite(560 + 60, sy + 92, false);
    // highlight keyed pixels
    body += R(560 + 60, sy + 92, 216, 216, { stroke: C.flags, sw: 3, r: 6, dash: '8 6' });
    body += sprite(1024 + 60, sy + 92, true);
    body += R(1488 + 60, sy + 92, 216, 216, { fill: C.tex, fop: 0.1, stroke: C.tex, sw: 1.5, r: 12 });
    body += T(1488 + 168, sy + 182, 'alphaMode', { size: 20, mono: true, fill: C.dim, anchor: 'middle' });
    body += T(1488 + 168, sy + 226, '"MASK"', { size: 30, mono: true, weight: 700, fill: C.tex, anchor: 'middle' });
    body += T(1488 + 168, sy + 262, 'cutoff 0.5', { size: 18, mono: true, fill: C.dim, anchor: 'middle' });

    return page(8, 'RE-VOLT FILE FORMATS', 'Texture pages', '.bmq low-res copies are ignored · env maps (gfx/envstill, envroll) are copied without a color key', body);
}

// ---------- 09 pipeline ----------

function pipeline()
{
    let body = '';
    const stages = [
        ['1', 'Find the game', C.count, ['Registry: SteamPath', 'libraryfolders.vdf', 'appmanifest_287310.acf', '→ common/Re-Volt']],
        ['2', 'Parse .w', C.center, ['1,452 meshes', '9,842 polygons', '21,006 vertices', '57 big cubes']],
        ['3', 'Group', C.radius, ['one node per big cube', '→ 57 nodes', 'cullable chunks', 'instead of 1,452 tiny ones']],
        ['4', 'Split materials', C.tex, ['texture page', '× blend mode', '× double-sided', '× env']],
        ['5', 'Write .glb', C.u16, ['flip Y and Z, × 0.01', 'reverse winding', '18,684 triangles', 'PNGs embedded']],
    ];
    const w = 312, gap = (1728 - 5 * w) / 4, y = 280, h = 470;
    stages.forEach(([n, title, color, lines], i) =>
    {
        const x = 96 + i * (w + gap);
        body += panel(x, y, w, h);
        body += `<circle cx="${x + 56}" cy="${y + 64}" r="30" fill="${color}" fill-opacity="0.18" stroke="${color}" stroke-width="2"/>`;
        body += T(x + 56, y + 75, n, { size: 30, weight: 700, fill: color, anchor: 'middle' });
        body += T(x + 28, y + 150, title, { size: 28, weight: 700, fill: color });
        body += L(x + 28, y + 178, x + w - 28, y + 178, { stroke: C.border, sw: 1.5 });
        lines.forEach((line, l) => { body += T(x + 28, y + 230 + l * 52, line, { size: 21, fill: C.text, mono: i == 0 && l < 3 }); });
        if (i < 4) body += L(x + w + 8, y + h / 2, x + w + gap - 10, y + h / 2, { stroke: C.accent, sw: 3, arrow: 'arrowA' });
    });

    // output
    const oy = 800;
    body += panel(96, oy, 1728, 180);
    body += T(128, oy + 52, 'OUTPUT', { size: 16, weight: 700, fill: C.faint, ls: 4 });
    const files = [
        ['assets/levels/market1/market1.glb', 'geometry, materials, textures', C.u16],
        ['assets/gfx/envstill.png', 'env map for the world', C.uv],
        ['assets/gfx/envroll.png', 'env map for the cars', C.uv],
    ];
    files.forEach(([path, desc, color], i) =>
    {
        const fx = 128 + i * 580;
        body += T(fx, oy + 108, path, { size: 22, mono: true, weight: 700, fill: color });
        body += T(fx, oy + 142, desc, { size: 19, fill: C.dim });
    });

    return page(9, 'THE CONVERTER', 'From Steam to .glb', 'Rust + egui · one click per level · runs on a worker thread', body);
}

// ---------- 10 glb ----------

function glbFile()
{
    let body = '';

    // file layout
    const y = 270, h = 120;
    const parts = [
        [96, 400, 'Header', '12 B', ['"glTF"', 'v2', 'length'], C.count],
        [496, 600, 'JSON chunk', 'length · "JSON"', ['scene graph', 'materials', 'accessors'], C.tex],
        [1096, 728, 'BIN chunk', 'length · "BIN\\0"', ['vertex data', 'indices', 'PNG textures'], C.u16],
    ];
    parts.forEach(([x, w, name, sub, items, color]) =>
    {
        body += R(x, y, w - 8, h, { fill: color, fop: 0.14, stroke: color, sw: 2, r: 12 });
        body += T(x + 24, y + 46, name, { size: 26, weight: 700, fill: color });
        body += T(x + w - 32, y + 46, sub, { size: 18, mono: true, fill: C.dim, anchor: 'end' });
        body += T(x + 24, y + 90, items.join('  ·  '), { size: 19, mono: true, fill: C.text });
    });

    // node tree
    const ty = 450;
    body += panel(96, ty, 640, 530);
    body += T(128, ty + 52, 'Node tree', { size: 28, weight: 700 });
    const tree = [
        [0, 'market1', C.text, 'root'],
        [1, 'cube_000', C.radius, 'mesh'],
        [2, 'primitive · market1a', C.tex, ''],
        [2, 'primitive · market1f_double', C.tex, ''],
        [2, 'primitive · market1c_env', C.tex, ''],
        [1, 'cube_001', C.radius, 'mesh'],
        [1, '…', C.dim, ''],
        [1, 'cube_056', C.radius, 'mesh'],
    ];
    tree.forEach(([depth, name, color, tag], i) =>
    {
        const nx = 140 + depth * 56, ny = ty + 116 + i * 50;
        if (depth > 0) body += P(`M ${nx - 36} ${ny - 40} L ${nx - 36} ${ny - 7} L ${nx - 10} ${ny - 7}`, { stroke: C.faint, sw: 1.5 });
        body += T(nx, ny, name, { size: 21, mono: true, fill: color, weight: depth == 0 ? 700 : 400 });
        if (tag) body += T(700, ny, tag, { size: 16, fill: C.faint, anchor: 'end', ls: 2 });
    });

    // primitive
    const px = 768, pw = 520;
    body += panel(px, ty, pw, 530);
    body += T(px + 32, ty + 52, 'Primitive', { size: 28, weight: 700 });
    const attributes = [
        ['POSITION', 'VEC3 · meters · min/max', C.center],
        ['NORMAL', 'VEC3', C.normal],
        ['TEXCOORD_0', 'VEC2', C.uv],
        ['COLOR_0', 'VEC4 · linear · pre-lit', C.color],
        ['COLOR_1', 'VEC4 · env tint (env only)', C.u16],
        ['indices', 'u32 · triangles', C.idx],
    ];
    attributes.forEach(([name, desc, color], i) =>
    {
        const ay = ty + 116 + i * 66;
        body += pill(px + 32, ay - 6, name, color, { w: 170, size: 18 });
        body += T(px + 222, ay, desc, { size: 19, fill: C.text });
    });

    // material
    const mx = 1320, mw = 504;
    body += panel(mx, ty, mw, 530);
    body += T(mx + 32, ty + 52, 'Material', { size: 28, weight: 700 });
    const json = [
        ['{', C.dim],
        ['  "name": "market1f_double_env",', C.text],
        ['  "baseColorTexture": { "index": 5 },', C.tex],
        ['  "alphaMode": "MASK",', C.flags],
        ['  "doubleSided": true,', C.center],
        ['  "extras": {', C.dim],
        ['    "revolt_env": true', C.u16],
        ['  }', C.dim],
        ['}', C.dim],
    ];
    json.forEach(([line, color], i) => { body += T(mx + 32, ty + 110 + i * 36, line, { size: 18, mono: true, fill: color }); });
    body += TS(mx + 32, ty + 452, [['additive polys: ', C.dim], ['"revolt_blend": "additive"', C.color, 400, true]], { size: 17 });
    body += T(mx + 32, ty + 490, 'extras: hints for what glTF can’t say', { size: 18, fill: C.dim });

    return page(10, 'THE CONVERTER', 'The .glb it writes', 'One self-contained file per level · loads straight into the editor', body);
}

// ---------- 11 .rim ----------

const PLANE_COLORS = ['#ff8f4d', '#ff7ad9', '#6fd08c', '#4fd8ff', '#b38cff'];

function rimFile()
{
    let body = '';

    // file layout
    const fy = 262;
    body += T(96, fy + 10, 'File · market1.rim · 462 bytes', { size: 22, weight: 600, fill: C.dim });
    const blocks = [['u16 count = 5', 2, C.count], ['plane 0', 92, PLANE_COLORS[0]], ['plane 1', 92, PLANE_COLORS[1]], ['plane 2', 92, PLANE_COLORS[2]], ['plane 3', 92, PLANE_COLORS[3]], ['plane 4', 92, PLANE_COLORS[4]]];
    let bx = 96;
    blocks.forEach(([name, bytes, color], i) =>
    {
        const w = i == 0 ? 200 : (1728 - 200) / 5;
        body += R(bx + 2, fy + 34, w - 4, 56, { fill: color, fop: 0.16, stroke: color, sw: 2, r: 10 });
        body += T(bx + w / 2, fy + 69, name, { size: 20, weight: 600, fill: color, anchor: 'middle', mono: i == 0 });
        body += T(bx + w / 2, fy + 118, `${bytes} B`, { size: 16, fill: C.dim, anchor: 'middle', mono: true });
        bx += w;
    });
    body += T(1824, fy + 10, '2 + 5 × 92 = 462 · checked byte-exact on all 13 .rim files', { size: 18, fill: C.dim, anchor: 'end' });

    // one record
    const ry = 520, ppb = 1728 / 92;
    body += T(96, ry - 70, 'One plane · 92 bytes', { size: 22, weight: 600, fill: C.dim });
    body += byteBar(96, ry, 110, ppb, [
        { name: 'flag', type: 'i32', bytes: 4, color: C.flags },
        { name: 'normal', type: 'f32 × 3', bytes: 12, color: C.center },
        { name: 'd', type: 'f32', bytes: 4, color: C.radius },
        { name: 'bounding box', type: 'f32 × 6', bytes: 24, color: C.bbox, sub: 6, subLabels: ['xlo', 'xhi', 'ylo', 'yhi', 'zlo', 'zhi'] },
        { name: 'corners', type: 'f32 × 3 × 4', bytes: 48, color: C.u16, sub: 4, subLabels: ['c0', 'c1', 'c2', 'c3'] },
    ], { nameSize: 22 });

    const cards = [
        [C.center, 'The plane', ['normal · p + d = 0', 'horizontal floors only', 'normal sign is mixed (±Y)']],
        [C.u16, 'The rectangle', ['4 corners on the plane', 'the area that mirrors', 'bbox says the same']],
        [C.tex, 'In the .glb', ['mirror_planes / mirror_N', 'empty nodes at the center', 'extras: normal, d, min/max,', 'corners, polygon count']],
    ];
    const cw = (1728 - 2 * 32) / 3;
    cards.forEach(([color, title, lines], i) =>
    {
        const cx = 96 + i * (cw + 32), cy = 720;
        body += panel(cx, cy, cw, 260);
        body += R(cx, cy, cw, 6, { fill: color, r: 3 });
        body += T(cx + 32, cy + 56, title, { size: 26, weight: 700, fill: color });
        lines.forEach((line, l) => { body += T(cx + 32, cy + 106 + l * 38, line, { size: 21, mono: i == 2 && l == 0, fill: C.text }); });
    });

    return page(11, 'RE-VOLT FILE FORMATS', 'Mirror planes · .rim', 'Where the shiny floors are · the polygons themselves carry the MIRROR flag', body);
}

// ---------- 12 market1 from above ----------

function mirrorMap()
{
    let body = '';
    // re-volt x/z of market1, z up on screen
    const planes = [
        [9000, 9450, -11500, -10500, 12, '0.00'], [9000, 9450, -8000, -7000, 12, '0.00'],
        [-3500, 5500, -19000, -12000, 548, '0'], [-3500, 9000, -12000, -7000, 610, '0'], [-3500, 11150, -7000, 2500, 633, '0'],
    ];
    const k = 0.0316, mx = 272, my = 306;
    const X = (x) => mx + (x + 3500) * k;
    const Y = (z) => my + (2500 - z) * k;

    body += panel(96, 252, 1000, 738);
    // level outline
    body += R(X(-3500), Y(2500), (17000 + 3500) * k, (2500 + 19000) * k, { stroke: C.faint, sw: 1.5, dash: '8 8', r: 4 });
    body += T(X(-3500), Y(2500) - 12, 'level bounds · 205 × 215 m', { size: 17, fill: C.faint });

    planes.forEach(([x0, x1, z0, z1, count], i) =>
    {
        const color = PLANE_COLORS[i];
        body += R(X(x0), Y(z1), (x1 - x0) * k, (z1 - z0) * k, { fill: color, fop: 0.22, stroke: color, sw: 2, r: 3 });
        if (i >= 2)
        {
            body += T(X((x0 + x1) / 2), Y((z0 + z1) / 2) - 4, `mirror_${i}`, { size: 22, weight: 700, fill: color, anchor: 'middle', mono: true });
            body += T(X((x0 + x1) / 2), Y((z0 + z1) / 2) + 26, `${count} polygons`, { size: 18, fill: C.text, anchor: 'middle' });
        }
    });
    // the two small ones - labels on the right
    [[0, -11000], [1, -7500]].forEach(([i, z]) =>
    {
        const color = PLANE_COLORS[i];
        body += L(X(9450) + 6, Y(z), X(9450) + 60, Y(z), { stroke: color, sw: 1.5 });
        body += T(X(9450) + 70, Y(z) + 7, `mirror_${i} · 12 polygons`, { size: 18, weight: 600, fill: color, mono: true });
    });
    // the two polygons on no plane
    [6313, 6188].forEach((x) => { body += `<circle cx="${X(x)}" cy="${Y(-12019)}" r="7" fill="${C.flags}" stroke="${C.bg}" stroke-width="2"/>`; });
    body += P(`M ${X(6313) + 10} ${Y(-12019) + 6} L ${X(8200)} ${Y(-14500)}`, { stroke: C.flags, sw: 1.5 });
    body += T(X(8200) + 8, Y(-14500) + 6, '2 polygons on no plane', { size: 18, weight: 600, fill: C.flags });
    body += T(X(8200) + 8, Y(-14500) + 32, '19 cm past the edge of mirror_3', { size: 16, fill: C.dim });

    // scale bar
    const sx = 620, sy = 950;
    body += L(sx, sy, sx + 5000 * k, sy, { stroke: C.text, sw: 3 });
    body += L(sx, sy - 8, sx, sy + 8, { stroke: C.text, sw: 2 });
    body += L(sx + 5000 * k, sy - 8, sx + 5000 * k, sy + 8, { stroke: C.text, sw: 2 });
    body += T(sx + 5000 * k + 14, sy + 7, '50 m', { size: 18, fill: C.text });
    body += T(1060, 290, 'TOP VIEW', { size: 15, weight: 700, fill: C.faint, anchor: 'end', ls: 3 });

    // legend
    const lx = 1136, lw = 688;
    body += panel(lx, 252, lw, 738);
    body += T(lx + 32, 306, 'PLANE', { size: 15, weight: 700, fill: C.faint, ls: 3 });
    body += T(lx + 200, 306, 'SIZE', { size: 15, weight: 700, fill: C.faint, ls: 3 });
    body += T(lx + 400, 306, 'HEIGHT', { size: 15, weight: 700, fill: C.faint, ls: 3 });
    body += T(lx + lw - 32, 306, 'POLYGONS', { size: 15, weight: 700, fill: C.faint, ls: 3, anchor: 'end' });
    const rows = [['4.5 × 10 m', '1.25 mm'], ['4.5 × 10 m', '1.25 mm'], ['90 × 70 m', '0'], ['125 × 50 m', '0'], ['146.5 × 95 m', '0']];
    rows.forEach(([size, height], i) =>
    {
        const y = 360 + i * 56;
        body += chip(lx + 32, y, PLANE_COLORS[i], 18);
        body += T(lx + 62, y, `mirror_${i}`, { size: 21, weight: 700, fill: PLANE_COLORS[i], mono: true });
        body += T(lx + 200, y, size, { size: 20, fill: C.text });
        body += T(lx + 400, y, height, { size: 20, fill: C.text, mono: true });
        body += T(lx + lw - 32, y, planes[i][4], { size: 20, fill: C.text, mono: true, anchor: 'end' });
    });
    body += L(lx + 32, 660, lx + lw - 32, 660, { stroke: C.border, sw: 1.5 });
    body += T(lx + 32, 700, '1,815 of 1,817 MIRROR polygons', { size: 22, weight: 700 });
    body += T(lx + 32, 732, 'found their plane', { size: 22, weight: 700 });
    body += T(lx + 32, 790, 'The material says which plane:', { size: 20, fill: C.dim });
    body += T(lx + 32, 830, 'market1f_mirror2', { size: 22, mono: true, weight: 700, fill: PLANE_COLORS[2] });
    body += T(lx + 32, 866, 'extras.revolt_mirror_plane = 2', { size: 20, mono: true, fill: C.text });
    body += T(lx + 32, 930, 'no plane → market1f_mirror + a warning', { size: 19, fill: C.flags });

    return page(12, 'THE CONVERTER', 'market1 from above', 'Five mirror rectangles · the supermarket floor is split into three, plus two small ones', body);
}

// ---------- 13 assignment ----------

function mirrorAssignment()
{
    let body = '';
    const y = 262, w = 500, h = 330, gap = (1728 - 3 * w) / 2;

    const steps = [
        ['1', 'Center', C.idx, ['average of the 3 or 4', 'polygon corners']],
        ['2', 'Inside a rectangle?', C.u16, ['center within the corners', '± 2 units (2 cm), all axes']],
        ['3', 'On the plane?', C.center, ['| n · c + d | ≤ 2 units', 'several hits: closest wins']],
    ];
    steps.forEach(([n, title, color, lines], i) =>
    {
        const x = 96 + i * (w + gap);
        body += panel(x, y, w, h);
        body += `<circle cx="${x + 50}" cy="${y + 52}" r="24" fill="${color}" fill-opacity="0.18" stroke="${color}" stroke-width="2"/>`;
        body += T(x + 50, y + 61, n, { size: 24, weight: 700, fill: color, anchor: 'middle' });
        body += T(x + 92, y + 61, title, { size: 26, weight: 700, fill: color });
        lines.forEach((line, l) => { body += T(x + 32, y + 270 + l * 32, line, { size: 20, fill: C.text, mono: i == 2 && l == 0 }); });
        if (i < 2) body += L(x + w + 10, y + h / 2, x + w + gap - 10, y + h / 2, { stroke: C.accent, sw: 3, arrow: 'arrowA' });
    });

    // 1: quad with its center
    {
        const x = 96, q = [[x + 150, y + 110], [x + 350, y + 100], [x + 370, y + 210], [x + 130, y + 220]];
        body += P(`M ${q.map((p) => p.join(' ')).join(' L ')} Z`, { fill: C.idx, fop: 0.14, stroke: C.idx, sw: 2 });
        q.forEach(([px, py]) => { body += `<circle cx="${px}" cy="${py}" r="6" fill="${C.idx}"/>`; });
        const cx = q.reduce((s, p) => s + p[0], 0) / 4, cy = q.reduce((s, p) => s + p[1], 0) / 4;
        q.forEach(([px, py]) => { body += L(px, py, cx, cy, { stroke: C.idx, sw: 1, dash: '4 5', op: 0.6 }); });
        body += `<circle cx="${cx}" cy="${cy}" r="9" fill="${C.text}"/>`;
        body += T(cx + 16, cy - 12, 'c', { size: 22, weight: 700, mono: true });
    }
    // 2: rectangle, one center inside, one outside
    {
        const x = 96 + w + gap;
        body += R(x + 90, y + 100, 300, 120, { fill: C.u16, fop: 0.14, stroke: C.u16, sw: 2, r: 4 });
        body += R(x + 84, y + 94, 312, 132, { stroke: C.u16, sw: 1, dash: '4 5', r: 6, op: 0.6 });
        body += `<circle cx="${x + 200}" cy="${y + 160}" r="9" fill="${C.center}"/>` + T(x + 216, y + 168, '✓', { size: 22, fill: C.center, weight: 700 });
        body += `<circle cx="${x + 440}" cy="${y + 160}" r="9" fill="${C.flags}"/>` + T(x + 440, y + 140, '✗', { size: 22, fill: C.flags, weight: 700, anchor: 'middle' });
    }
    // 3: side view - plane line, center with its distance
    {
        const x = 96 + 2 * (w + gap);
        body += L(x + 60, y + 190, x + 440, y + 190, { stroke: C.center, sw: 3 });
        body += T(x + 446, y + 197, 'plane', { size: 16, fill: C.center });
        body += L(x + 120, y + 190, x + 120, y + 130, { stroke: C.center, sw: 2, arrow: 'arrowY' });
        body += T(x + 132, y + 128, 'n', { size: 20, weight: 700, fill: C.center, mono: true });
        body += `<circle cx="${x + 280}" cy="${y + 178}" r="9" fill="${C.text}"/>`;
        body += L(x + 300, y + 178, x + 300, y + 190, { stroke: C.text, sw: 1.5 });
        body += T(x + 312, y + 172, '≤ 2 cm', { size: 16, fill: C.text });
        body += `<circle cx="${x + 230}" cy="${y + 120}" r="9" fill="${C.flags}"/>` + T(x + 246, y + 126, 'too far', { size: 16, fill: C.flags });
    }

    // results over all levels
    const results = [
        ['frontend', 1, 222, 0], ['garden1', 2, 256, 0], ['markar', 1, 210, 0], ['market1', 5, 1815, 2], ['market2', 1, 694, 0],
        ['muse1', 9, 702, 26], ['muse2', 6, 506, 1], ['nhood2', 5, 50, 1], ['toy2', 8, 655, 4], ['toylite', 5, 1518, 22],
    ];
    const ty = 630;
    body += panel(96, ty, 1728, 360);
    body += T(128, ty + 46, 'ALL LEVELS WITH MIRRORS', { size: 15, weight: 700, fill: C.faint, ls: 3 });
    body += TS(1792, ty + 46, [['■ ', C.center], ['on a plane    ', C.dim], ['■ ', C.flags], ['on no plane', C.dim]], { size: 17, anchor: 'end' });
    results.forEach(([name, planes, ok, missing], i) =>
    {
        const col = Math.floor(i / 5), row = i % 5;
        const x = 128 + col * 864, ry = ty + 100 + row * 54;
        const total = ok + missing;
        body += T(x, ry, name, { size: 21, weight: 600, mono: true });
        body += T(x + 140, ry, `${planes} plane${planes > 1 ? 's' : ''}`, { size: 18, fill: C.dim });
        const bw = 380;
        body += R(x + 250, ry - 17, bw, 20, { fill: C.border, fop: 0.5, r: 5 });
        body += R(x + 250, ry - 17, bw * ok / total, 20, { fill: C.center, fop: 0.85, r: 5 });
        if (missing > 0) body += R(x + 250 + bw * ok / total, ry - 17, Math.max(4, bw * missing / total), 20, { fill: C.flags, r: 3 });
        body += T(x + 650, ry, `${ok.toLocaleString('en-US')}`, { size: 19, mono: true, fill: C.text });
        if (missing > 0) body += T(x + 730, ry, `+${missing}`, { size: 19, mono: true, fill: C.flags });
    });

    return page(13, 'THE CONVERTER', 'Which polygon mirrors where?', 'Every MIRROR polygon gets the plane it lies on · prepared for later, screen space reflections need only the flag', body);
}

// ---------- 14 how re-volt mirrors ----------

// side view car standing on (or hanging below) the floor at fy - dir -1 = up, +1 = mirrored down
function car(cx, fy, dir, s, color, o = {})
{
    const y = (u) => fy + dir * u * s;
    const x = (v) => cx + v * s;
    let out = R(x(-90), Math.min(y(15), y(55)), 180 * s, 40 * s, { fill: color, fop: o.fop ?? 0.85, stroke: color, sw: 2, r: 8 * s, dash: o.dash });
    out += P(`M ${x(-55)} ${y(55)} L ${x(-35)} ${y(82)} L ${x(35)} ${y(82)} L ${x(55)} ${y(55)} Z`, { fill: color, fop: (o.fop ?? 0.85) * 0.7, stroke: color, sw: 2, dash: o.dash });
    for (const v of [-60, 60]) out += `<circle cx="${x(v)}" cy="${y(15)}" r="${15 * s}" fill="${C.bg}" stroke="${color}" stroke-width="2" ${o.dash ? `stroke-dasharray="${o.dash}"` : ''}/>`;
    return out;
}

function eye(x, y, label, below = false)
{
    return `<circle cx="${x}" cy="${y}" r="14" fill="${C.text}"/><circle cx="${x + 4}" cy="${y}" r="6" fill="${C.bg}"/>` + T(x - (below ? 20 : 0), below ? y + 42 : y - 26, label, { size: 17, weight: 600, anchor: 'middle' });
}

function revoltMirror()
{
    let body = '';
    const fy = 640;

    body += panel(96, 252, 1060, 738);
    body += T(128, 300, 'SIDE VIEW', { size: 15, weight: 700, fill: C.faint, ls: 3 });

    // the mirrored copy below the floor, drawn first
    body += `<defs><linearGradient id="fade" gradientUnits="userSpaceOnUse" x1="0" y1="${fy}" x2="0" y2="${fy + 240}"><stop offset="0" stop-color="${C.bbox}" stop-opacity="0.45"/><stop offset="0.62" stop-color="${C.bbox}" stop-opacity="0.45"/><stop offset="1" stop-color="${C.bbox}" stop-opacity="0"/></linearGradient></defs>`;
    body += R(820, fy, 80, 240, { fill: 'url(#fade)', r: 2 });
    body += car(680, fy, 1, 1, C.u16, { fop: 0.3, dash: '6 5' });
    body += T(680, fy + 130, 'mirrored copy · drawn first', { size: 18, fill: C.dim, anchor: 'middle' });

    // the real scene
    body += R(820, fy - 240, 80, 240, { fill: C.bbox, fop: 0.75, r: 2 });
    for (const u of [60, 120, 180]) body += L(820, fy - u, 900, fy - u, { stroke: C.bg, sw: 3 });
    body += T(860, fy - 254, 'shelf', { size: 17, fill: C.bbox, anchor: 'middle' });
    body += car(680, fy, -1, 1, C.u16);
    body += T(680, fy - 100, 'car', { size: 17, fill: C.u16, anchor: 'middle' });

    // height limit
    body += L(300, fy - 150, 960, fy - 150, { stroke: C.faint, sw: 1.5, dash: '6 6' });
    body += T(440, fy - 160, 'mirror distance · 5.12 m · above it fades out', { size: 16, fill: C.dim });

    // floor: opaque - mirror - opaque
    body += R(130, fy - 3, 170, 10, { fill: C.dim, r: 2 });
    body += R(960, fy - 3, 160, 10, { fill: C.dim, r: 2 });
    body += R(300, fy - 3, 660, 10, { fill: C.accent, fop: 0.45, r: 2 });
    body += T(312, fy + 34, 'MIRROR polygons', { size: 17, weight: 600, fill: C.accent });
    body += T(312, fy + 58, 'drawn on top at 60 % (mix)', { size: 16, fill: C.accent });
    body += T(1040, fy + 34, 'opaque floor', { size: 17, weight: 600, fill: C.dim, anchor: 'middle' });
    body += T(1040, fy + 58, 'hides the copy', { size: 16, fill: C.dim, anchor: 'middle' });
    body += T(215, fy + 34, 'opaque floor', { size: 17, weight: 600, fill: C.dim, anchor: 'middle' });

    // what the eye sees
    body += eye(220, 360, 'eye');
    body += L(220, 360, 560, fy, { stroke: C.text, sw: 2.5 });
    body += L(560, fy, 620, fy + 49, { stroke: C.text, sw: 2.5, dash: '6 5' });
    body += `<circle cx="620" cy="${fy + 49}" r="6" fill="${C.text}"/>`;
    body += L(560, fy, 620, fy - 49, { stroke: C.u16, sw: 2, dash: '2 6' });
    body += T(256, 340, 'the eye looks through the floor', { size: 17, fill: C.text });
    body += T(256, 364, 'and sees the mirrored car', { size: 17, fill: C.text });

    // steps
    const sx = 1188, sw = 636;
    body += panel(sx, 252, sw, 738);
    const steps = [
        ['1', 'Mirror the scene', C.bbox, ['world, cars and objects above a', '.rim plane, flipped below the floor']],
        ['2', 'Draw the copy first', C.u16, ['as if the room went on downward', 'up to the mirror distance']],
        ['3', 'Draw the floor on top', C.accent, ['MIRROR polygons translucent', 'with the mix from the .inf (0.6)']],
        ['4', 'The rest hides it', C.dim, ['opaque floor and walls cover the', 'copy through the z-buffer']],
    ];
    steps.forEach(([n, title, color, lines], i) =>
    {
        const y = 300 + i * 168;
        body += `<circle cx="${sx + 52}" cy="${y + 22}" r="22" fill="${color}" fill-opacity="0.18" stroke="${color}" stroke-width="2"/>`;
        body += T(sx + 52, y + 30, n, { size: 22, weight: 700, fill: color, anchor: 'middle' });
        body += T(sx + 92, y + 31, title, { size: 25, weight: 700, fill: color });
        lines.forEach((line, l) => { body += T(sx + 92, y + 72 + l * 30, line, { size: 19, fill: C.text }); });
    });
    body += T(sx + 32, 960, 'No render to texture in 1999 · probably, not checked against code', { size: 16, fill: C.faint });

    return page(14, 'REFLECTIONS', 'How Re-Volt mirrors', 'A second, flipped copy of the scene under a see-through floor', body);
}

// ---------- 15 planar reflection vs ssr ----------

function reflectionMethods()
{
    let body = '';
    const fy = 480;

    const textBlock = (x, y, rows) =>
    {
        let s = '';
        rows.forEach(([label, color, lines], i) =>
        {
            const ry = y + i * 92;
            s += T(x, ry, label, { size: 15, weight: 700, fill: color, ls: 3 });
            lines.forEach((line, l) => { s += T(x, ry + 30 + l * 28, line, { size: 19, fill: C.text, mono: label == 'DATA' }); });
        });
        return s;
    };

    // planar
    {
        const x0 = 96, w = 844;
        body += panel(x0, 252, w, 738);
        body += R(x0, 252, w, 6, { fill: C.center, r: 3 });
        body += T(x0 + 32, 304, 'Planar reflection', { size: 30, weight: 700, fill: C.center });
        body += T(x0 + w - 32, 304, 'LATER', { size: 15, weight: 700, fill: C.center, ls: 3, anchor: 'end' });

        body += R(x0 + 40, fy - 3, 160, 8, { fill: C.dim, r: 2 });
        body += R(x0 + 200, fy - 3, 520, 8, { fill: C.accent, fop: 0.45, r: 2 });
        body += R(x0 + 720, fy - 3, 90, 8, { fill: C.dim, r: 2 });
        body += T(x0 + 810, fy + 30, 'clip plane = mirror plane', { size: 15, fill: C.dim, anchor: 'end' });
        body += car(x0 + 560, fy, -1, 0.7, C.u16);

        body += eye(x0 + 110, 360, 'camera');
        body += L(x0 + 110, 360, x0 + 390, fy, { stroke: C.text, sw: 2 });
        // mirrored camera below the plane
        body += `<circle cx="${x0 + 110}" cy="600" r="14" fill="none" stroke="${C.center}" stroke-width="2" stroke-dasharray="4 4"/>`;
        body += T(x0 + 140, 606, 'mirrored camera', { size: 17, weight: 600, fill: C.center });
        body += L(x0 + 110, 360, x0 + 110, 586, { stroke: C.faint, sw: 1.5, dash: '3 6' });
        body += L(x0 + 124, 594, x0 + 520, 445, { stroke: C.center, sw: 2, dash: '6 5', arrow: true });
        // texture
        body += R(x0 + 560, 540, 70, 56, { fill: C.center, fop: 0.18, stroke: C.center, sw: 2, r: 6 });
        body += T(x0 + 645, 563, 'reflection', { size: 16, fill: C.center });
        body += T(x0 + 645, 585, 'texture', { size: 16, fill: C.center });
        body += L(x0 + 580, 536, x0 + 540, fy + 10, { stroke: C.center, sw: 2, arrow: true });

        body += textBlock(x0 + 32, 668, [
            ['HOW', C.dim, ['mirror the camera, render into a texture, map it', 'onto the _mirror polygons (screen space UVs)']],
            ['+  /  −', C.center, ['exact, also off screen, closest to the original', 'one extra scene pass per visible plane']],
            ['DATA', C.dim, ['mirror_N planes · revolt_mirror_plane', 'mirror_mix']],
        ]);
    }

    // ssr
    {
        const x0 = 980, w = 844;
        body += panel(x0, 252, w, 738);
        body += R(x0, 252, w, 6, { fill: C.uv, r: 3 });
        body += T(x0 + 32, 304, 'Screen space reflection', { size: 30, weight: 700, fill: C.uv });
        body += T(x0 + w - 32, 304, 'FIRST', { size: 15, weight: 700, fill: C.uv, ls: 3, anchor: 'end' });

        body += R(x0 + 40, fy - 3, 770, 8, { fill: C.accent, fop: 0.45, r: 2 });
        // view frustum = what is on screen
        const cam = [x0 + 100, 340];
        body += P(`M ${cam[0]} ${cam[1]} L ${x0 + 810} 380 L ${x0 + 810} ${fy} L ${x0 + 300} ${fy} Z`, { fill: C.uv, fop: 0.06, stroke: C.uv, sw: 1, dash: '5 6' });
        body += T(x0 + 800, 372, 'on screen', { size: 15, fill: C.uv, anchor: 'end' });
        body += car(x0 + 650, fy, -1, 0.7, C.u16);
        body += eye(cam[0], cam[1], 'camera', true);

        // ray 1: hits the car
        body += L(cam[0], cam[1], x0 + 560, fy, { stroke: C.text, sw: 2 });
        for (let x = x0 + 575; x <= x0 + 610; x += 12) body += `<circle cx="${x}" cy="${fy - (x - x0 - 560) * 0.304}" r="3" fill="${C.uv}"/>`;
        body += T(x0 + 622, fy - 28, '✓', { size: 22, weight: 700, fill: C.center });
        // ray 2: leaves the screen
        body += L(cam[0], cam[1], x0 + 330, fy, { stroke: C.text, sw: 2 });
        for (let x = x0 + 345; x <= x0 + 540; x += 16) body += `<circle cx="${x}" cy="${fy - (x - x0 - 330) * 0.609}" r="3" fill="${C.uv}"/>`;
        body += T(x0 + 552, 340, '✗', { size: 22, weight: 700, fill: C.flags });
        body += T(x0 + 576, 338, 'leaves the screen', { size: 16, fill: C.flags });
        body += T(x0 + 576, 360, 'nothing to sample', { size: 16, fill: C.flags });
        body += T(x0 + 240, 560, 'march the reflected ray through the depth buffer', { size: 16, fill: C.uv });

        body += textBlock(x0 + 32, 668, [
            ['HOW', C.dim, ['one fullscreen pass after the scene: follow the', 'reflected ray on screen, take the color it hits']],
            ['+  /  −', C.uv, ['cheap, any surface, no extra scene pass', 'only what is on screen: gaps at edges, under cars']],
            ['DATA', C.dim, ['revolt_mirror (where) · mirror_mix (how strong)', '']],
        ]);
    }

    return page(15, 'REFLECTIONS', 'Planar reflection vs SSR', 'Two ways for rustl · the glb already has the data for both', body);
}

// ---------- 16 atlas seams ----------

function atlasSeams()
{
    let body = '';
    const cell = 52, cols = 10, rows = 4;
    const tileA = ['#3e7d4f', '#47895a'], tileB = ['#9a3d4d', '#ab4a5b'];

    const grid = (gx, gy) =>
    {
        let s = '';
        for (let r = 0; r < rows; r++)
        {
            for (let c = 0; c < cols; c++)
            {
                const colors = c < 5 ? tileA : tileB;
                s += `<rect x="${gx + c * cell}" y="${gy + r * cell}" width="${cell - 2}" height="${cell - 2}" rx="4" fill="${colors[(r + c) % 2]}"/>`;
            }
        }
        s += T(gx + 2.5 * cell, gy - 14, 'tile A', { size: 17, weight: 600, fill: C.center, anchor: 'middle' });
        s += T(gx + 7.5 * cell, gy - 14, 'tile B · neighbour in the atlas', { size: 17, weight: 600, fill: C.flags, anchor: 'middle' });
        return s;
    };

    // how the ground looks: two polygons with tile A next to each other
    const strip = (sx, sy, sw, seam) =>
    {
        let s = R(sx, sy, sw, 52, { fill: tileA[0], r: 8 });
        s += R(sx + sw / 2, sy, sw / 2, 52, { fill: tileA[1], r: 0 });
        if (seam) s += R(sx + sw / 2 - 3, sy, 6, 52, { fill: tileB[1] });
        s += R(sx, sy, sw, 52, { stroke: C.border, sw: 2, r: 8 });
        return s;
    };

    const panels = [
        [96, 'Before', C.flags, false, 'UV edge exactly on the texel edge', 'the filter reads 50 % of tile B', 'thin seam between the polygons'],
        [980, 'After', C.center, true, 'UV edge half a texel inward', 'the filter stays inside tile A', 'no seam'],
    ];
    panels.forEach(([x0, title, color, inset, l1, l2, l3]) =>
    {
        const w = 844;
        body += panel(x0, 252, w, 560);
        body += R(x0, 252, w, 6, { fill: color, r: 3 });
        body += T(x0 + 32, 302, title, { size: 30, weight: 700, fill: color });

        const gx = x0 + 32 + (w - 64 - cols * cell) / 2, gy = 362;
        body += grid(gx, gy);

        // polygon edge in the texture and the 2×2 bilinear footprint of a sample on it
        const edge = gx + 5 * cell - 1 - (inset ? cell / 2 : 0);
        body += L(edge, gy - 4, edge, gy + rows * cell + 2, { stroke: C.text, sw: 4 });
        if (inset) body += L(gx + 5 * cell - 1, gy - 4, gx + 5 * cell - 1, gy + rows * cell + 2, { stroke: C.text, sw: 1.5, dash: '5 6', op: 0.6 });
        const sy = gy + 1.5 * cell;
        const fx = inset ? edge - cell / 2 : edge - cell / 2;
        body += R(fx, sy - cell / 2, cell, cell, { stroke: '#ffd76e', sw: 3, r: 6 });
        body += `<circle cx="${edge}" cy="${sy}" r="8" fill="#ffd76e"/>`;
        body += T(edge, gy + rows * cell + 30, 'polygon edge (uv)', { size: 16, fill: C.text, anchor: 'middle' });

        body += TS(x0 + 32, gy + rows * cell + 72, [[l1 + ' · ', C.text], [l2, color, 600]], { size: 21 });
        body += strip(x0 + 32, gy + rows * cell + 98, w - 64, !inset);
        body += T(x0 + w / 2, gy + rows * cell + 180, l3, { size: 17, fill: C.dim, anchor: 'middle' });
    });
    body += T(960, 535, '→', { size: 48, weight: 700, fill: C.accent, anchor: 'middle' });

    // bottom: why, fix, result
    const cards = [
        [C.u16, 'Why', ['texture pages are atlases: many', 'tiles side by side, no gaps between']],
        [C.uv, 'Fix', ['u ± 0.5 / 256 toward the uv center', 'per polygon · tile 1.6 % smaller']],
        [C.center, 'garden1', ['5,701 of 5,974 polygons sat on a', 'texel edge · mipmaps stay on']],
    ];
    const cw = (1728 - 64) / 3;
    cards.forEach(([color, title, lines], i) =>
    {
        const cx = 96 + i * (cw + 32), cy = 840;
        body += panel(cx, cy, cw, 150);
        body += R(cx, cy, 6, 150, { fill: color, r: 3 });
        body += T(cx + 32, cy + 46, title, { size: 24, weight: 700, fill: color });
        lines.forEach((line, l) => { body += T(cx + 32, cy + 86 + l * 30, line, { size: 19, fill: C.text, mono: i == 1 && l == 0 }); });
    });

    return page(16, 'THE CONVERTER', 'Seams between atlas tiles', 'Bilinear filtering reads across the tile border · half a texel inset fixes it', body);
}

// ---------- 17 bots: where the waypoints come from ----------

// a numbered circle with a title - the step style of the pipeline
function step(x, y, n, title, color, r = 22, size = 25)
{
    return `<circle cx="${x}" cy="${y}" r="${r}" fill="${color}" fill-opacity="0.18" stroke="${color}" stroke-width="2"/>`
        + T(x, y + r * 0.36, n, { size: r, weight: 700, fill: color, anchor: 'middle' })
        + T(x + r + 18, y + size * 0.36, title, { size, weight: 700, fill: color });
}

function botData()
{
    let body = '';
    const stages = [
        ['1', '.fan file', C.idx, ['levels/<level>/<level>.fan', 'one AI node per cross', 'section of the track', 'garden1: 336 · 76 bytes each']],
        ['2', 'Converter', C.center, ['convert/track.rs', '× 0.01 → metres', '(x, −y, −z) → glTF', 'links without the −1']],
        ['3', 'Scene', C.tex, ['level object extras', 'revolt_ai_nodes', 'one JSON string', 'beside .taz / .pan / .tri']],
        ['4', 'App code', C.u16, ['track.rs  Track::ai', 'bot.rs  drives on it', 'race.rs  puts cars', 'back onto it']],
    ];
    const w = 396, gap = (1728 - 4 * w) / 3, y = 250, h = 390;
    stages.forEach(([n, title, color, lines], i) =>
    {
        const x = 96 + i * (w + gap);
        body += panel(x, y, w, h);
        body += `<circle cx="${x + 56}" cy="${y + 64}" r="30" fill="${color}" fill-opacity="0.18" stroke="${color}" stroke-width="2"/>`;
        body += T(x + 56, y + 75, n, { size: 30, weight: 700, fill: color, anchor: 'middle' });
        body += T(x + 28, y + 150, title, { size: 28, weight: 700, fill: color });
        body += L(x + 28, y + 178, x + w - 28, y + 178, { stroke: C.border, sw: 1.5 });
        lines.forEach((line, l) => { body += T(x + 28, y + 228 + l * 44, line, { size: 21, fill: C.text, mono: l == 0 || (i == 2 && l == 1) }); });
        if (i < 3) body += L(x + w + 8, y + h / 2, x + w + gap - 10, y + h / 2, { stroke: C.accent, sw: 3, arrow: 'arrowA' });
    });

    // the record
    const oy = 676;
    body += panel(96, oy, 1728, 314);
    body += T(128, oy + 46, 'ONE AI NODE · 76 BYTES', { size: 16, weight: 700, fill: C.faint, ls: 4 });
    const fields = [
        { name: 'hdr', type: 'u8×4', bytes: 4, color: C.faint },
        { name: 'line', type: 'f32', bytes: 4, color: C.center },
        { name: 'finish', type: 'f32', bytes: 4, color: C.faint },
        { name: '?', type: 'f32', bytes: 4, color: C.faint },
        { name: '?', type: 'i32', bytes: 4, color: C.faint },
        { name: 'speeds?', type: 'i32 × 2', bytes: 8, color: C.faint },
        { name: 'prev', type: 'i32 × 2', bytes: 8, color: C.idx },
        { name: 'next', type: 'i32 × 2', bytes: 8, color: C.idx },
        { name: '?', type: 'i32', bytes: 4, color: C.faint },
        { name: 'right edge', type: 'vec3', bytes: 12, color: C.uv },
        { name: '?', type: 'i32', bytes: 4, color: C.faint },
        { name: 'left edge', type: 'vec3', bytes: 12, color: C.x },
    ];
    body += byteBar(128, oy + 84, 96, 21.5, fields, { nameSize: 20 });

    const ly = oy + 270;
    body += chip(128, ly, C.center) + T(156, ly, 'racing line · 0 = left edge, 1 = right edge', { size: 19 });
    body += chip(640, ly, C.idx) + T(668, ly, 'prev / next · up to 2 each (forks)', { size: 19 });
    body += chip(1050, ly, C.uv) + T(1078, ly, 'edges · glTF metres', { size: 19 });
    body += chip(1340, ly, C.faint) + T(1368, ly, 'not decoded · not used', { size: 19, fill: C.dim });

    return page(17, 'BOTS', 'Where the waypoints come from', 'The AI nodes of the original - converted once, read by the app code', body);
}

// ---------- 18 bots: following the line ----------

function bez(p, t)
{
    const u = 1 - t;
    return [0, 1].map((k) => u * u * u * p[0][k] + 3 * u * u * t * p[1][k] + 3 * u * t * t * p[2][k] + t * t * t * p[3][k]);
}

function bezDir(p, t)
{
    const u = 1 - t;
    const d = [0, 1].map((k) => 3 * u * u * (p[1][k] - p[0][k]) + 6 * u * t * (p[2][k] - p[1][k]) + 3 * t * t * (p[3][k] - p[2][k]));
    const n = Math.hypot(d[0], d[1]);
    return [d[0] / n, d[1] / n];
}

const pts = (list) => list.map(([x, y]) => `${x.toFixed(1)},${y.toFixed(1)}`).join(' ');
const lerp2 = (a, b, t) => [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];

function lineFollowing()
{
    let body = '';
    body += panel(96, 252, 1080, 738);
    body += T(128, 300, 'TOP VIEW', { size: 15, weight: 700, fill: C.faint, ls: 3 });

    // the track: a bend, half width hw - left of the driving direction is (dy, −dx) on screen
    const curve = [[190, 800], [500, 812], [640, 470], [1050, 440]];
    const hw = 88;
    const side = (t, s) => { const c = bez(curve, t); const d = bezDir(curve, t); return [c[0] + d[1] * hw * s, c[1] - d[0] * hw * s]; };
    const left = [], right = [];
    for (let t = 0; t <= 1.0001; t += 0.02) { left.push(side(t, 1)); right.push(side(t, -1)); }
    body += `<polygon points="${pts(left.concat(right.reverse()))}" fill="#162036"/>`;
    body += `<polyline points="${pts(left)}" fill="none" stroke="${C.dim}" stroke-width="2"/>`;
    body += `<polyline points="${pts(right)}" fill="none" stroke="${C.dim}" stroke-width="2"/>`;
    const centre = [];
    for (let t = 0; t <= 1.0001; t += 0.02) centre.push(bez(curve, t));
    body += `<polyline points="${pts(centre)}" fill="none" stroke="${C.faint}" stroke-width="1.5" stroke-dasharray="2 8"/>`;

    // the nodes: cross sections, irregular spacing, racing line value per node
    const nodes = [[0.0, 0.32], [0.11, 0.28], [0.25, 0.36], [0.37, 0.55], [0.48, 0.72], [0.6, 0.68], [0.72, 0.48], [0.85, 0.3], [0.97, 0.3]];
    const cross = nodes.map(([t, line]) => ({ l: side(t, 1), r: side(t, -1), line, t }));
    const at = (n, line) => lerp2(n.l, n.r, line);
    cross.forEach((n, i) =>
    {
        body += L(n.l[0], n.l[1], n.r[0], n.r[1], { stroke: C.faint, sw: 2, dash: '6 6' });
        body += `<circle cx="${n.l[0]}" cy="${n.l[1]}" r="6" fill="${C.x}"/><circle cx="${n.r[0]}" cy="${n.r[1]}" r="6" fill="${C.uv}"/>`;
        body += T(n.l[0] + (n.l[0] - n.r[0]) * 0.18, n.l[1] + (n.l[1] - n.r[1]) * 0.18 + 6, String(i), { size: 17, weight: 700, fill: C.dim, anchor: 'middle', mono: true });
    });

    // the segment the car is on
    const seg = [at(cross[2], 0.5), at(cross[3], 0.5)];
    body += L(seg[0][0], seg[0][1], seg[1][0], seg[1][1], { stroke: C.accent, sw: 10, op: 0.35 });

    // racing line and the bot's own line
    const racing = cross.map((n) => at(n, n.line));
    const own = cross.map((n) => at(n, Math.min(0.9, n.line + 0.14)));
    body += `<polyline points="${pts(racing)}" fill="none" stroke="${C.center}" stroke-width="4"/>`;
    body += `<polyline points="${pts(own)}" fill="none" stroke="${C.u16}" stroke-width="3" stroke-dasharray="10 8"/>`;

    // the car on its own line between node 2 and 3, the target further along
    const carPos = lerp2(own[2], own[3], 0.35);
    const heading = (() => { const d = [own[3][0] - own[2][0], own[3][1] - own[2][1]]; const n = Math.hypot(...d); return [d[0] / n, d[1] / n]; })();
    const angle = Math.atan2(heading[1], heading[0]) * 180 / Math.PI;
    const target = lerp2(own[5], own[6], 0.2);
    body += `<path d="M ${carPos[0]} ${carPos[1]} Q ${carPos[0] + heading[0] * 150} ${carPos[1] + heading[1] * 150} ${target[0]} ${target[1]}" fill="none" stroke="${C.accent}" stroke-width="3" marker-end="url(#arrowA)"/>`;
    body += L(carPos[0], carPos[1], target[0], target[1], { stroke: C.text, sw: 1.5, dash: '4 6', op: 0.7 });
    body += `<g transform="translate(${carPos[0]} ${carPos[1]}) rotate(${angle})">${R(-34, -18, 68, 36, { fill: C.u16, fop: 0.9, r: 8 })}${R(8, -14, 16, 28, { fill: C.bg, fop: 0.5, r: 4 })}</g>`;
    body += `<circle cx="${target[0]}" cy="${target[1]}" r="11" fill="none" stroke="${C.text}" stroke-width="3"/><circle cx="${target[0]}" cy="${target[1]}" r="4" fill="${C.text}"/>`;

    // labels
    body += L(target[0] + 14, target[1] + 8, 860, 760, { stroke: C.text, sw: 1.2, op: 0.5 });
    body += T(870, 768, 'look ahead point', { size: 19, weight: 600 });
    body += T(870, 794, '2.5 m + 0.35 s × speed', { size: 17, fill: C.dim });
    const segMid = lerp2(seg[0], seg[1], 0.35);
    body += L(segMid[0] + 8, segMid[1] + 10, 640, 880, { stroke: C.accent, sw: 1.2, op: 0.6 });
    body += T(648, 902, 'segment 2 → 3 of the centre line, t = 0.35', { size: 17, fill: C.accent });
    body += L(racing[7][0], racing[7][1] + 4, 960, 640, { stroke: C.center, sw: 1.2, op: 0.6 });
    body += T(960, 664, 'racing line (.fan)', { size: 19, weight: 600, fill: C.center, anchor: 'middle' });
    body += L(own[2][0] - 6, own[2][1] + 4, 300, 900, { stroke: C.u16, sw: 1.2, op: 0.6 });
    body += T(300, 924, 'own line: + offset + wander', { size: 19, weight: 600, fill: C.u16, anchor: 'middle' });
    body += chip(150, 960, C.x) + T(176, 960, 'left edge', { size: 17, fill: C.dim }) + chip(290, 960, C.uv) + T(316, 960, 'right edge', { size: 17, fill: C.dim });
    body += T(1140, 960, 'driving direction →', { size: 17, fill: C.dim, anchor: 'end' });

    // steps
    const sx = 1208, sw = 616;
    body += panel(sx, 252, sw, 738);
    const steps = [
        ['1', 'Where am I', C.accent, ['the nearest centre segment', 'node → next, first around the', 'last one · height counts 3 ×']],
        ['2', 'My own line', C.u16, ['racing line + own offset (±8 %)', '+ wander: a new aim beside it', 'every 1.5-5 s, ±18 % of the width']],
        ['3', 'Look ahead', C.text, ['a point on the own line', '2.5 m + 0.35 s × speed ahead', '(at most 14 m)']],
        ['4', 'Steer', C.accent, ['the wheel angle of the circle', 'through that point (pure pursuit)', '÷ full lock → input_override']],
    ];
    steps.forEach(([n, title, color, lines], i) =>
    {
        const y = 304 + i * 160;
        body += step(sx + 52, y + 22, n, title, color);
        lines.forEach((line, l) => { body += T(sx + 92, y + 70 + l * 30, line, { size: 19, fill: C.text }); });
    });
    body += T(sx + 32, 960, 'Forks: every bot keeps one random pick', { size: 17, fill: C.faint });

    return page(18, 'BOTS', 'Following the line', 'Racing line of the AI nodes, an own offset, steering at a point ahead', body);
}

// ---------- 19 bots: the speed plan ----------

function speedPlan()
{
    let body = '';

    // sampling
    body += panel(96, 252, 820, 330);
    body += T(128, 300, 'THE LINE AHEAD · EVERY 2 M', { size: 15, weight: 700, fill: C.faint, ls: 3 });
    const curve = [[150, 512], [420, 532], [560, 342], [880, 334]];
    const samples = [];
    for (let t = 0; t <= 1.0001; t += 1 / 26) samples.push(bez(curve, t));
    body += `<polyline points="${pts(samples)}" fill="none" stroke="${C.center}" stroke-width="3" opacity="0.6"/>`;
    samples.forEach(([x, y]) => { body += `<circle cx="${x}" cy="${y}" r="4.5" fill="${C.center}"/>`; });
    const [a, b, c] = [samples[8], samples[11], samples[14]];
    body += L(a[0], a[1], b[0], b[1], { stroke: C.u16, sw: 3 });
    body += L(b[0], b[1], c[0], c[1], { stroke: C.u16, sw: 3 });
    for (const [p, n] of [[a, 'a'], [b, 'b'], [c, 'c']]) body += `<circle cx="${p[0]}" cy="${p[1]}" r="9" fill="${C.u16}"/>` + T(p[0] + 16, p[1] + 26, n, { size: 20, weight: 700, fill: C.u16, mono: true });
    body += T(c[0] + 56, c[1] + 52, '6 m back · 6 m ahead', { size: 18, fill: C.u16 });
    body += T(128, 556, 'curvature = angle(ab, bc) ÷ 6 m', { size: 21, mono: true, fill: C.text });

    // formulas
    body += panel(944, 252, 880, 330);
    body += T(976, 300, 'TARGET SPEED', { size: 15, weight: 700, fill: C.faint, ls: 3 });
    const rows = [
        ['corner', 'corner = √(18 m/s² × pace ÷ curvature)', C.center],
        ['brake', 'allowed = √(corner² + 2 × 16 m/s² × distance)', C.flags],
        ['target', 'target = min over the line ahead', C.u16],
        ['', '        ≤ 1.1 × top speed × pace', C.u16],
        ['drive', 'below target: full throttle · above: brake', C.text],
    ];
    rows.forEach(([, text, color], i) => { body += T(976, 346 + i * 42, text, { size: 21, mono: true, fill: color }); });
    body += T(976, 560, 'pace: Hard 0.95-1.0 per bot, ±0.04 jitter every few s', { size: 17, fill: C.dim });

    // the profile
    const gx0 = 200, gx1 = 1770, gy0 = 900, gy1 = 712;
    body += panel(96, 612, 1728, 378);
    body += T(128, 656, 'SPEED ALONG THE LINE AHEAD', { size: 15, weight: 700, fill: C.faint, ls: 3 });
    const X = (d) => gx0 + (gx1 - gx0) * d / 120;
    const Y = (v) => gy0 - (gy0 - gy1) * v / 32;
    body += L(gx0, gy0, gx1, gy0, { stroke: C.border, sw: 2 });
    for (let d = 0; d <= 120; d += 20) body += T(X(d), gy0 + 28, `${d} m`, { size: 15, fill: C.faint, anchor: 'middle', mono: true });
    for (const v of [10, 20, 30]) body += T(gx0 - 16, Y(v) + 5, `${v}`, { size: 15, fill: C.faint, anchor: 'end', mono: true }) + L(gx0, Y(v), gx1, Y(v), { stroke: C.border, sw: 1, dash: '3 8' });
    body += T(gx0 - 16, Y(32) - 6, 'm/s', { size: 15, fill: C.faint, anchor: 'end' });

    const top = 28;
    const corners = [[40, 13, 10], [94, 19, 8]];
    const plan = (d) =>
    {
        let v = top;
        for (const [cd, cv, len] of corners)
        {
            if (d <= cd) v = Math.min(v, Math.sqrt(cv * cv + 2 * 16 * (cd - d)));
            else if (d <= cd + len) v = Math.min(v, cv);
        }
        return v;
    };
    // before: node to node - kinks looked like curves
    const noise = (d) => { const s = Math.sin(d * 12.9898) * 43758.5453; return s - Math.floor(s); };
    const before = [];
    for (let d = 0; d <= 120; d += 3) before.push([X(d), Y(Math.max(9, plan(d) - (noise(d) > 0.45 ? noise(d * 3.1) * 15 : 0)))]);
    body += `<polyline points="${pts(before)}" fill="none" stroke="${C.flags}" stroke-width="2" stroke-dasharray="6 6" opacity="0.75"/>`;
    const after = [];
    for (let d = 0; d <= 120; d += 0.5) after.push([X(d), Y(plan(d))]);
    body += `<polyline points="${pts(after)}" fill="none" stroke="${C.center}" stroke-width="4"/>`;
    body += L(gx0, Y(top), gx1, Y(top), { stroke: C.dim, sw: 1.5, dash: '10 8' });
    body += T(gx1, Y(top) - 12, 'top speed × pace', { size: 16, fill: C.dim, anchor: 'end' });
    for (const [cd, cv, len] of corners)
    {
        body += R(X(cd), gy1 - 6, X(cd + len) - X(cd), gy0 - gy1 + 6, { fill: C.u16, fop: 0.07, r: 4 });
        body += T(X(cd + len / 2), gy1 + 12, 'curve', { size: 15, fill: C.u16, anchor: 'middle' });
        body += T(X(cd + len / 2), Y(cv) + 26, `${cv} m/s`, { size: 15, fill: C.center, anchor: 'middle', mono: true });
    }
    body += T(X(36), Y(24.2), 'brake ramp 16 m/s²', { size: 16, fill: C.center });
    body += chip(gx0, 966, C.center) + T(gx0 + 26, 966, 'now: line sampled every 2 m, curve over ±6 m', { size: 16 });
    body += chip(gx0 + 560, 966, C.flags) + T(gx0 + 586, 966, 'before: node to node - every kink of the line was a curve (13 ↔ 30 m/s every 0.5 s)', { size: 16, fill: C.dim });

    return page(19, 'BOTS', 'How fast', 'The slowest the curves ahead allow - +22 % distance after the line was smoothed', body);
}

// ---------- 20 bots: when something goes wrong ----------

function botRecovery()
{
    let body = '';
    const stages = [
        ['1', 'Stuck', C.u16, '1.2 s', ['throttle on, but slower', 'than 0.6 m/s'], ['reverse for 1 s,', 'steering the other way']],
        ['2', 'Lost', C.flags, '2 s', ['upside down · 3 m beyond', 'the edge · out of every', 'zone (.taz) · or 3 reverses', 'in a row'], ['onto the racing line', 'of the next node,', 'upright, standing']],
        ['3', 'No progress', C.tex, '7 s', ['less than 4 m along the', 'track (race score), e.g.', 'stuck in the garden1 water'], ['1, 2, 3 … nodes further', 'ahead each time · the count', 'clears after 50 m']],
    ];
    const w = 540, gap = (1728 - 3 * w) / 2, y = 252, h = 520;
    stages.forEach(([n, title, color, time, when, then], i) =>
    {
        const x = 96 + i * (w + gap);
        body += panel(x, y, w, h);
        body += step(x + 56, y + 62, n, title, color, 30, 30);
        body += pill(x + w - 150, y + 62, time, color, { w: 118 });
        body += L(x + 28, y + 118, x + w - 28, y + 118, { stroke: C.border, sw: 1.5 });
        body += T(x + 32, y + 162, 'WHEN', { size: 15, weight: 700, fill: C.faint, ls: 3 });
        when.forEach((line, l) => { body += T(x + 32, y + 198 + l * 32, line, { size: 21 }); });
        body += T(x + 32, y + 350, 'THEN', { size: 15, weight: 700, fill: color, ls: 3 });
        then.forEach((line, l) => { body += T(x + 32, y + 386 + l * 32, line, { size: 21, fill: color }); });
        if (i < 2) body += L(x + w + 8, y + h / 2, x + w + gap - 10, y + h / 2, { stroke: C.accent, sw: 3, arrow: 'arrowA' });
    });

    const oy = 800;
    body += panel(96, oy, 1728, 190);
    body += T(128, oy + 48, 'PUT BACK', { size: 15, weight: 700, fill: C.faint, ls: 3 });
    body += T(128, oy + 92, 'VehicleController::place(scene, position, heading)', { size: 21, mono: true, fill: C.u16 });
    body += T(128, oy + 126, 'upright, at a standstill, out of walls first · 0.3 m above the racing line · facing to the node after', { size: 19 });
    body += T(128, oy + 160, 'log: race: Phat Slug put back onto the track (upside down)', { size: 18, mono: true, fill: C.dim });
    body += T(1792, oy + 92, 'garden1 · 7 bots · 90 s', { size: 18, fill: C.dim, anchor: 'end' });
    body += T(1792, oy + 126, 'about one reset per bot per 80 s', { size: 21, weight: 600, fill: C.center, anchor: 'end' });

    return page(20, 'BOTS', 'When something goes wrong', 'Three checks, from free yourself to put back further ahead', body);
}

// ---------- 21 the field and the race ----------

function raceField()
{
    let body = '';

    // opponents by rating
    body += panel(96, 252, 920, 500);
    body += T(128, 300, 'OPPONENTS BY THE RATING OF YOUR CAR', { size: 15, weight: 700, fill: C.faint, ls: 3 });
    const ratings = [
        ['ROOKIE', ['rc', 'mite', 'phat', 'moss', 'mud', 'beatall', 'volken', 'tc6']],
        ['AMATEUR', ['dino', 'candy', 'gencar', 'tc4', 'mouse']],
        ['ADVANCED', ['flag', 'tc2', 'r5', 'tc5', 'sgt']],
        ['SEMI-PRO', ['tc3', 'adeon', 'fone', 'tc1']],
        ['PRO', ['cougar', 'sugo', 'toyeca', 'amw']],
    ];
    const picked = new Set(['cougar', 'sugo', 'toyeca', 'tc3', 'adeon', 'fone', 'tc1']);
    ratings.forEach(([name, cars], i) =>
    {
        const x = 128 + i * 176;
        const used = i >= 3;
        body += T(x + 70, 350, name, { size: 15, weight: 700, fill: used ? C.text : C.faint, ls: 2, anchor: 'middle' });
        cars.forEach((car, c) =>
        {
            const color = car == 'amw' ? C.u16 : picked.has(car) ? C.center : C.faint;
            body += pill(x, 392 + c * 36, car, color, { w: 140, size: 16 });
        });
    });
    body += L(128 + 4 * 176 + 70, 560, 128 + 3 * 176 + 70, 560, { stroke: C.accent, sw: 3, arrow: 'arrowA' });
    body += T(128 + 3.5 * 176 + 70, 594, 'then down', { size: 16, fill: C.accent, anchor: 'middle' });
    body += T(128, 700, 'amw (pro), 8 cars: cougar, sugo, toyeca + 4 semi-pro · random within a rating', { size: 18 });
    body += T(128, 730, 'never: rotor, panga, trolley, the wincars, ufo, q · glbs loaded at the start', { size: 18, fill: C.dim });
    body += chip(760, 300, C.u16) + T(784, 300, 'you', { size: 16, fill: C.dim }) + chip(840, 300, C.center) + T(864, 300, 'picked', { size: 16, fill: C.dim });

    // the grid
    const gx = 1046, gw = 778;
    body += panel(gx, 252, gw, 738);
    body += T(gx + 32, 300, 'START GRID · OWN LAYOUT', { size: 15, weight: 700, fill: C.faint, ls: 3 });
    const cx = gx + 200, top = 372, ppm = 52;
    body += R(cx - 2.6 * ppm, top - 30, 5.2 * ppm, 8.4 * 1.2 * ppm + 60, { fill: '#162036', r: 12 });
    for (let i = 0; i < 8; i++)
    {
        const x = cx + (i % 2 == 0 ? -1 : 1) * ppm, y = top + i * 1.2 * ppm * 0.95;
        const you = i == 7;
        const color = you ? C.u16 : C.radius;
        body += R(x - 16, y - 2, 32, 54, { fill: color, fop: 0.85, r: 7 });
        body += T(x + (i % 2 == 0 ? -34 : 34), y + 32, String(i), { size: 17, weight: 700, fill: C.dim, anchor: i % 2 == 0 ? 'end' : 'start', mono: true });
        if (you) body += T(x + 34, y + 58, 'you', { size: 17, weight: 700, fill: C.u16 });
    }
    body += L(cx + 2.9 * ppm, top + 30, cx + 2.9 * ppm, top - 40, { stroke: C.text, sw: 2.5, arrow: 'arrowW' });
    body += T(cx + 2.9 * ppm + 14, top - 20, 'driving', { size: 16, fill: C.dim });
    const info = ['slot 0 = STARTPOS of the .inf', '1.2 m per slot, ±1.0 m to the side', 'bots in front, the players behind', 'floor of each slot from the .ncp', 'revolt_grid', 'on the start car'];
    info.forEach((line, l) => { body += T(cx + 3.4 * ppm, 520 + l * 34, line, { size: 18, fill: l >= 4 ? C.dim : C.text, mono: l == 4 }); });

    // every frame
    const fy = 776;
    body += panel(96, fy, 920, 214);
    body += T(128, fy + 40, 'EVERY FRAME, EVERY RACER', { size: 15, weight: 700, fill: C.faint, ls: 3 });
    const boxes = [
        ['Progress', C.count, ['.taz zones', '.pan nodes', 'laps, place']],
        ['Driver', C.u16, ['you: bindings', 'bot: bot.rs', 'later: remote']],
        ['input_override', C.tex, ['throttle, brake', 'steer, handbrake', 'or none']],
        ['Vehicle', C.center, ['the same', 'physics as', 'the player']],
    ];
    const bw = 200, bg = (856 - 4 * bw) / 3;
    boxes.forEach(([title, color, lines], i) =>
    {
        const x = 128 + i * (bw + bg);
        body += R(x, fy + 58, bw, 132, { fill: color, fop: 0.08, stroke: color, sw: 1.5, r: 12 });
        body += T(x + bw / 2, fy + 90, title, { size: 20, weight: 700, fill: color, anchor: 'middle', mono: i == 2 });
        lines.forEach((line, l) => { body += T(x + bw / 2, fy + 122 + l * 24, line, { size: 17, anchor: 'middle' }); });
        if (i < 3) body += L(x + bw + 4, fy + 124, x + bw + bg - 6, fy + 124, { stroke: C.accent, sw: 2.5, arrow: 'arrowA' });
    });

    return page(21, 'THE RACE', 'The field', 'Opponents by rating, an own grid, one race logic for players and bots', body);
}

// ---------- write + render ----------

const pages = [
    ['01_level_folder', levelFolder], ['02_w_file', worldFile], ['03_mesh', meshRecord], ['04_polygon_vertex', polygonRecord],
    ['05_hex_dump', hexDump], ['06_polygon_flags', polygonFlags], ['07_coordinates', coordinates], ['08_textures', textures],
    ['09_pipeline', pipeline], ['10_glb', glbFile], ['11_mirror_planes', rimFile], ['12_mirror_map', mirrorMap], ['13_mirror_assignment', mirrorAssignment], ['14_revolt_mirror', revoltMirror], ['15_reflection_methods', reflectionMethods], ['16_atlas_seams', atlasSeams],
    ['17_bot_data', botData], ['18_bot_line', lineFollowing], ['19_bot_speed', speedPlan], ['20_bot_recovery', botRecovery], ['21_race_field', raceField],
];

for (const [name, make] of pages)
{
    const svg = join(DIR, `${name}.svg`);
    writeFileSync(svg, make());

    if (existsSync(EDGE))
    {
        execFileSync(EDGE, [
            '--headless=new', '--disable-gpu', '--hide-scrollbars', '--force-device-scale-factor=2', '--window-size=1920,1080',
            `--screenshot=${join(DIR, `${name}.png`)}`, pathToFileURL(svg).href,
        ], { stdio: 'ignore' });
    }
    console.log(name);
}
