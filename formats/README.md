# Re-Volt file formats

Notes from taking apart the PC version of Re-Volt (Steam, AppID 287310, `revolt.exe` 1,104,128 bytes, MD5 `4283a91552238e3e9431b74ff62ab023`).

Every layout below was parsed against all files of the game. "Checked" means every file of that type parses to its last byte. Anything not checked is marked as such.

## Conventions

- Little endian. `f32` = IEEE 754 float, `vec3` = 3 × `f32`, `uv` = 2 × `f32`.
- Coordinates: x right, **y down**, z forward. Right-handed. To glTF (y up): `(x, -y, -z)`, a 180° turn around x, no mirroring.
- Front faces are **clockwise**. Flip the winding for glTF / OpenGL.
- Units: a car wheel has a radius of about 10, the supermarket is about 20,500 × 21,500. `0.01` per unit gives an RC car in meters.
- Colors are `u32` 0xAARRGGBB, stored as B, G, R, A.
- File names in the game folder mix upper and lower case (`MARKET1.taz`, `market1a.BMP`). Look them up case-insensitively.

## Level folder

`levels/<level>/`

| File | Content | Here |
|---|---|---|
| `<level>.w` | world mesh | [.w](#w) |
| `<level>a.bmp` … `<level>j.bmp` | texture pages 0–9 | [Textures](#textures) |
| `*.bmq` | 128 × 128 copies of the pages | ignore |
| `*.prm` | models placed by the `.fin` | [.prm / .m](#prm--m) |
| `<level>.rim` | mirror planes | [.rim](#rim) |
| `<level>.fin` | model instances | [.fin](#fin) |
| `<level>.fob` | game objects | [.fob](#fob) |
| `<level>.inf` | level info, text | [.inf](#inf) |
| `<level>.ncp`, `<model>.ncp` | collision | [.ncp](#ncp) |
| `<level>.taz` | track zones | [.taz](#taz) |
| `<level>.pan` | position nodes | [.pan](#pan) |
| `<level>.fan` | AI nodes | [.fan](#fan) |
| `<level>.tri` | triggers | [.tri](#tri) |
| `.cam` `.lit` `.vis` `.fld` | cameras, lights, visiboxes, force fields | not covered yet |
| `reversed/` | the same level driven backwards | |

## .w

World mesh. No header. Checked: all 19 levels.

```
i32       mesh_count
Mesh      meshes[mesh_count]
i32       big_cube_count
BigCube   big_cubes[big_cube_count]
i32       tex_anim_count
TexAnim   tex_anims[tex_anim_count]
u32       env_colors[env_count]      env_count is not stored: number of polygons with FACE_ENV
```

### Mesh

```
vec3      center                     bounding sphere
f32       radius
f32       xlo, xhi, ylo, yhi, zlo, zhi   bounding box, min/max per axis (not min xyz, max xyz)
u16       polygon_count
u16       vertex_count
Polygon   polygons[polygon_count]
Vertex    vertices[vertex_count]
```

### Polygon (60 bytes)

```
u16       flags
i16       texture                    page 0–9, -1 = none; with FACE_TEXANIM: index of the texture animation
u16       indices[4]                 into this mesh's vertices; triangles ignore indices[3]
u32       colors[4]                  per corner, pre-baked lighting; alpha only used with FACE_TRANSLUCENT
uv        uvs[4]                     per corner, 0–1, origin top left
```

UVs and colors belong to the polygon, not to the vertex, so every corner becomes its own vertex in a GPU mesh.

### Vertex (24 bytes)

```
vec3      position
vec3      normal
```

### Polygon flags

| Bit | Value | Name | Meaning |
|---|---|---|---|
| 0 | 1 | QUAD | 4 corners, else 3 |
| 1 | 2 | DOUBLE | double-sided |
| 2 | 4 | TRANSLUCENT | alpha from the vertex colors |
| 7 | 128 | MIRROR | reflective floor, see [.rim](#rim) |
| 8 | 256 | TRANSL_TYPE | with TRANSLUCENT: additive blending |
| 9 | 512 | TEXANIM | `texture` is a texture animation index |
| 10 | 1024 | NOENV | |
| 11 | 2048 | ENV | env mapped, has an entry in `env_colors` |
| 12 | 4096 | CLOTH | |
| 13 | 8192 | SKIP | not drawn |

### BigCube

Bounding sphere around a group of meshes, used for culling. In every level each mesh belongs to exactly one big cube.

```
vec3      center
f32       radius
i32       mesh_count
i32       mesh_indices[mesh_count]
```

### TexAnim

```
i32       frame_count
Frame     frames[frame_count]

Frame (40 bytes):
i32       texture                    page
f32       delay
uv        uvs[4]
```

## .prm / .m

Models: `.prm` in the level and car folders, `.m` in `models/`. The same body as a `.w` mesh, without the 40 bytes of bounds:

```
u16       polygon_count
u16       vertex_count
Polygon   polygons[polygon_count]
Vertex    vertices[vertex_count]
```

**Levels of detail**: a file can hold several meshes in a row. All car wheels (104 files) have 5, from detailed to coarse (rc: 44, 38, 32, 23, 17 polygons); every other car, level and model file has 1.

Level models use the texture pages of their level. Checked: all `.prm` used by `.fin` files and the `.m` files read so far.

## Textures

- `<level>a.bmp` is page 0, `b` page 1, and so on. Plain Windows BMP, 256 × 256, 24 bit, rows bottom up.
- **Pure black `000000` is transparent** (color key).
- The pages are atlases: many tiles side by side, without gaps. Most UVs sit exactly on texel edges (garden1: 5,701 of 5,974 polygons), so bilinear filtering reads into the neighbor tile. Moving each corner half a texel toward the UV center of its polygon removes the seams.
- Env maps: `gfx/envstill.bmp` (world), `gfx/envroll.bmp` (cars), `gfx/envfront.bmp` (set by `ENVSTILL` in some `.inf` files).

## .rim

Mirror planes. Checked: all 13 `.rim` files.

```
u16       plane_count
Plane     planes[plane_count]

Plane (92 bytes):
i32       flag                       1 in all files, meaning unknown
vec3      normal                     plane: normal · p + d = 0
f32       d
f32       xlo, xhi, ylo, yhi, zlo, zhi   bounding box
vec3      corners[4]                 the mirror rectangle
```

- All 43 planes in the game are horizontal, all with flag 1. The normal points up in some files and down in others.
- The reflective polygons are the ones with FACE_MIRROR. Assigned to a plane by "center inside the rectangle (±2 units) and at most 2 units from the plane", all but a few polygons per level find their plane (market1: 1,815 of 1,817).
- The game most likely draws the scene a second time, mirrored below the floor, and the MIRROR polygons translucent on top (`MIRRORS` mix in the `.inf`). *Not checked against code.*

## .fin

Model instances. Checked: all 15 `.fin` files.

```
i32       instance_count
Instance  instances[instance_count]

Instance (72 bytes):
char      name[9]                    model file name without extension, upper case, cut to 8 characters, zero padded
i8        color[3]                   r, g, b offset added to the model colors
u32       env_color
u8        priority
u8        flags
u8        padding[2]
f32       lod_bias
vec3      position
vec3      right, up, look            local axes in world space
```

- A world point is `position + right·x + up·y + look·z`, so the rotation matrix has `right`, `up`, `look` as columns. All 1,535 matrices in the game are pure rotations (determinant +1, unit axes).
- Names are cut: `FOUNTAIN` is `fountain1.prm`, `MILK_ROW` matches both `milk_row_ff.prm` and `milk_row_nf.prm`.
- An instance collides only if a `.ncp` with the model name exists in the level folder (`palmtree.ncp`, `fountain1.ncp`). Plants without one are driven through.

Instance flags (from the [Re-Volt Blender add-on](https://github.com/Re-Volt/re-volt-addon), `io_revolt/common.py`):

| Value | Name |
|---|---|
| 1 | ENV |
| 2 | HIDE |
| 4 | NO_MIRROR |
| 8 | NO_LIGHTS |
| 16 | SET_MODEL_RGB |
| 32 | NO_OBJECT_COLLISION |
| 64 | NO_CAMERA_COLLISION |

## .fob

Game objects. Checked: all 19 `.fob` files.

```
i32       object_count
Object    objects[object_count]

Object (56 bytes):
i32       type                       see the table below
i32       flags[4]                   meaning depends on the type
vec3      position
vec3      up, look                   local axes in world space, as in the .fin; right = up × look
```

All 887 objects of the game have unit length, perpendicular `up` and `look` vectors.

### Object types

Source: the object table of the built-in editor in `revolt.exe`, 72 bytes per entry (a number that is not the type, a pointer to the name, pointers to the labels of the flags, ...). The index in that table is the type. Checked against the objects of all levels (pickups everywhere, trolleys and bottles in the supermarket, rain and lightning on the ship, and so on).

| Type | Name | flags[0..3] |
|---|---|---|
| 0 | Spinning Barrel | speed |
| 1 | Beachball | |
| 2 | Planet | name, orbit, orbit speed, spin speed |
| 3 | Plane | speed, radius, bank |
| 4 | Copter | x range, y range, z range, y offset |
| 5 | Dragon | |
| 6 | Water | |
| 7 | Trolley | |
| 8 | Boat | type |
| 9 | Speedup | width, low speed (mph), high speed (mph), time (s) |
| 10 | Radar | |
| 11 | Balloon | |
| 12 | Horse | |
| 13 | Train | |
| 14 | Strobe | type, sequence number, sequence count |
| 15 | Football | |
| 16 | Spark Generator | type, average speed, speed variation, frequency |
| 17 | Space Man | |
| 18–29 | – | no editor entry (weapons and other runtime objects, probably) |
| 30 | Pickup | |
| 31 | – | |
| 32 | Flappage | |
| 33 | Laser | width, random, object |
| 34–35 | – | |
| 36 | Wobbly Cone | |
| 37 | Probe Logo | |
| 38 | Clouds | |
| 39 | Name Entry | |
| 40 | Sprinkler | ID |
| 41 | Sprinkler Hose | ID |
| 42 | Object Thrower | ID, object, speed, reuse |
| 43 | BasketBall | |
| 44 | TrackScreen | |
| 45 | Clock | |
| 46 | CarBox | car ID |
| 47 | Stream | model |
| 48 | Cup | type |
| 49 | 3D Sound | name, range, random play |
| 50 | Star | type (0: the global pickup - only out in races with pickups on, 1: practice / stunt star - only in practice mode and the stunt arena) |
| 51 | – | |
| 52 | Tumbleweed | |
| 53 | Small Screen | |
| 54 | Lantern | |
| 55 | Skybox | level |
| 56 | Slider | ID |
| 57 | Bottle | stop |
| 58 | Bucket | |
| 59 | Cone | |
| 60 | Can | |
| 61 | Lilo | |
| 62 | – | |
| 63 | Rain | |
| 64 | Lightning | |
| 65 | Ship light | |
| 66 | Packet | |
| 67 | ABC Block | |
| 68 | Water Box | x range, y range, z range |
| 69 | Water Ripples | model |
| 70 | Gari flag | garyness |
| 71 | Dolphin | |
| 72 | Garden Fog | |
| 73 | Fog Box | x range, y range, z range |

### Models

The game picks the model of an object in code. Matched by name:

| Type | Model |
|---|---|
| 0 Spinning Barrel | `models/barrel.m` |
| 1 Beachball | `models/beachball.m` |
| 2 Planet | name 0–10: `mercury`, `venus`, `earth`, `mars`, `jupiter`, `saturn`, `uranus`, `neptune`, `pluto`, `moon`, `rings`; 11: no model, probably the sun |
| 3 Plane | `models/plane.m` (`plane2.m`: propeller) |
| 4 Copter | `models/copter.m` (`copter2.m`, `copter3.m`: rotors) |
| 5 Dragon | `models/dragon1.m` (`dragon2.m`, `dragon3.m`: parts) |
| 6 Water | `models/water.m` |
| 7 Trolley | `cars/trolley/trollbod.m` |
| 8 Boat | type 0, 1: `boat1.m`, `boat2.m` |
| 9–13 | `speedup`, `radar`, `balloon`, `horse`, `train` (`train2`, `train3`: wheels) |
| 14 Strobe | type 0–2: `light1`–`light3` |
| 15, 17, 30 | `football`, `spaceman`, `pickup` |
| 32 Flappage | `flap` |
| 36, 37 | `cone`, `probe` |
| 39, 44, 45, 46, 53 | `levels/frontend/`: `namestand`, `bigscreen`, `clockbody`, `carbox`, `smallscreen` |
| 40 Sprinkler | `spbase`, in the garden `spbasegarden` (`sphead`, `spheadgarden`: heads) |
| 41, 43 | `sphose`, `basketball` |
| 47 Stream | model 0 "Hood Stream": `stream`; model 1 "Toytanic Pool": unknown model |
| 48 Cup | type 0–3: `levels/frontend/cup01`–`cup04` |
| 50, 52, 54 | `star`, `tumble_weed`, `lantern` |
| 56–59 | `slider`, `bottle`, `bucket`, `trafficcone` (nhood2: `n2trafficcone`) |
| 65–67 | `shiplight`, `packet` (`packet1.m`: unknown), `abcblock` |
| 69 Water Ripples | model 1 "Toytanic Pool": `poolwater`, 2–5 "Garden 1"–"Garden 4": `gardenwater1`–`gardenwater4` |

The rest have no model (sounds, sparks, fog, clouds, rain, ...).

The names in quotes are the option labels of the editor in the exe (a list next to the 3D sound names).

- The planet numbers are confirmed by their orbits: the rings (10) orbit 5 (Saturn), the moon (9) orbits 2 (Earth). The others are matched by name only.
- `boat2.m` does not fit the .m layout (the only file that does not).
- **Page 24 is `levels/market1/trolley.bmp`**, the wire basket of the trolley (`cars/trolley/TrollBod.m`, 5 double sided quads; its `Parameters.txt` has `TPAGE "NONE"`). Found by laying the UVs over every candidate; the exe has the path `levels\market1	rolley.bmp` hard coded, so every level takes the market1 file. Only market1 has the file; market2 has trolleys too - the converter takes the market1 one there, like the game.
- **The shared models use the texture pages of the level they are placed in**, plus a page offset of the object init: the model loader adds the offset to every textured polygon. Offsets: beachball (type 1) and speedup (9) 27, dragon1 / dragon2 (5) 31, all others 0. So `beachball.m` (page 0 in the file) uses page 27 = `gfx/fxpage1.bmp`: its polygons have tiny UVs (1-9 texels) on red, yellow, blue and green spots of the fx page - the stripes. `spbasegarden.m` uses page 9 of garden1, the planets page 7 of the museum.
- **Pages beyond a..j**, loaded by hard coded `push page, push file`: 22 `gfx/font.bmp`, 23 `gfx/spru.bmp` (`flap.m` uses 23), 26 `gfx/shadow.bmp`, 27-29 `gfx/fxpage1-3.bmp`, 30 `gfx/loading.bmp`, 31 one page per level (`levels/toy2/dragon.bmp`, `levels/toylite/water.bmp`, `gfx/clouds.bmp`), 32 `levels/market1/trolley.bmp` (the car model `TrollBod.m` uses 24 for it).
- Water Ripples (69): the option labels "Garden 1"–"Garden 4" for model 2–5 confirm `gardenwater1.m`–`gardenwater4.m`, the sizes fit too. These models are flat, untextured and white. The game draws the water in code.
- 3D Sound: `name` indexes a list of sound names that is also in the exe ("Garden stream", "Garden animal frog", ...).

### Skybox

- Six images `sky_ft`, `sky_bk`, `sky_lt`, `sky_rt`, `sky_tp`, `sky_bt` (.bmp, 256 × 256, 24 bit) in nhood1, ship1 and ship2. No color key: black is part of the night sky of ship2.
- The Skybox object (55) picks the set with its `level` flag: 0 "Toytanic day" (ship1), 1 "Toytanic night" (ship2), 2 "Wild West" (no images in the game), 3 "Neighborhood" (nhood1). stunts uses 3.
- Which way up `sky_tp` / `sky_bt` are and whether the side images are mirrored is not checked yet.

### Clouds

- Levels without sky images but with a Clouds object (38): garden1, market2, bot_bat, nhood1_battle, nhood2, wild_west1, wild_west2.
- The background is the `FOGCOLOR` of the `.inf` (garden1: 80 144 192, the blue of the game).
- `gfx/clouds.bmp` (256 × 256) holds 6 clouds and a sun glow on black. Pixel rectangles (x0, y0, x1, y1): (16, 17, 88, 69), (128, 6, 165, 52), (168, 1, 253, 127), (22, 80, 44, 110), (67, 78, 122, 117), (5, 138, 127, 238), sun (128, 130, 254, 254). The edges are hard: every sprite pixel is at least 160 bright, black around it. The sun is a light blue disc (196) with a bright center.
- In the game the clouds and the sun sit all around up to the horizon, like on a dome. How many and where is decided in code.
- `gfx/<level>.bmp` are the preview images of the level menu, not sky.
- Indoor levels show their `FOGCOLOR` as background too: market1 has no ceiling at all (neither in the .w nor in the .fin), above the lamps the game shows its dark brown 72 62 50.

### Objects per level

| Level | Objects |
|---|---|
| bot_bat | Pickup 20, Clouds 1, 3D Sound 4, Star 4 |
| frontend | Wobbly Cone 6, Name Entry 1, TrackScreen 1, Clock 1, CarBox 29, Cup 4, Small Screen 5 |
| garden1 | Pickup 26, Clouds 1, Sprinkler 3, 3D Sound 8, Star 2, Water Ripples 4, Dolphin 1, Garden Fog 1 |
| markar | Pickup 15, 3D Sound 1, Star 7 |
| market1 | Trolley 2, Pickup 33, 3D Sound 9, Star 2, Bottle 14, Packet 3 |
| market2 | Trolley 2, Pickup 14, Clouds 1, 3D Sound 3, Star 2, Slider 2, Bottle 9, Packet 11 |
| muse1 | Pickup 43, Laser 10, 3D Sound 1, Star 2 |
| muse2 | Spinning Barrel 1, Planet 12, Strobe 28, Spark Generator 2, Pickup 34, Laser 4, 3D Sound 6, Star 2 |
| Muse_Bat | Planet 4, Strobe 16, Pickup 4, Star 4 |
| nhood1 | Strobe 10, Pickup 27, Sprinkler 4, Sprinkler Hose 4, Object Thrower 2, 3D Sound 8, Star 2, Skybox 1, Cone 4 |
| nhood1_battle | Pickup 26, Clouds 1, Sprinkler 1, Sprinkler Hose 1, BasketBall 1, 3D Sound 4, Star 9 |
| nhood2 | Pickup 31, Clouds 1, Sprinkler 3, Sprinkler Hose 3, Stream 1, 3D Sound 7, Star 2, Cone 2 |
| ship1 | Beachball 2, Pickup 28, Stream 1, 3D Sound 4, Star 2, Skybox 1, Water Box 1 |
| ship2 | Beachball 2, Pickup 26, Stream 1, 3D Sound 6, Star 2, Skybox 1, Bottle 3, Rain 1, Lightning 1, Ship light 12, Water Box 1 |
| stunts | Pickup 18, 3D Sound 1, Star 20, Skybox 1 |
| toy2 | Beachball 1, Plane 2, Copter 1, Dragon 1, Train 1, Pickup 20, Star 2 |
| toylite | Beachball 1, Plane 1, Copter 1, Water 1, Boat 2, Radar 1, Balloon 1, Horse 2, Spark Generator 1, Pickup 14, 3D Sound 1, Star 2, ABC Block 15 |
| wild_west1 | Pickup 24, Clouds 1, 3D Sound 5, Star 2, Tumbleweed 3, Gari flag 2 |
| wild_west2 | Pickup 31, Clouds 1, Object Thrower 1, 3D Sound 10, Star 2, Tumbleweed 6, Lantern 8, Bottle 2, Gari flag 2, Fog Box 1 |

## .inf

Text, one `KEY value ;comment` per line, keys case-insensitive, strings in single quotes.

| Key | Value | Meaning |
|---|---|---|
| `NAME` | string | name in the menus |
| `STARTPOS` / `STARTPOSREV` | x y z | start position, forward / reversed |
| `STARTROT` / `STARTROTREV` | 0–1 | start rotation in turns |
| `STARTGRID` / `STARTGRIDREV` | int | starting grid type |
| `FARCLIP` | units | draw distance |
| `FOGSTART` | units | fog start, a bit less than `FARCLIP` |
| `FOGCOLOR` | r g b | 0–255, also the background |
| `VERTFOGSTART` / `VERTFOGEND` | units | vertical fog |
| `WORLDRGBPER` / `MODELRGBPER` / `INSTANCERGBPER` | percent | brightness of the baked colors |
| `MIRRORS` | type mix intensity distance | mirror floors, e.g. `0 0.60 0 512` |
| `REDBOOK` | start end | CD audio track range |
| `MP3` | string | music file |
| `ENVSTILL` | string | env map, e.g. `'gfx\envfront.bmp'` |
| `ROCK` | 4 numbers | ship levels, meaning unknown |
| `SHADOWBOX` | number | one level, meaning unknown |

## .ncp

Collision: convex polygons stored as planes, each with a surface material. `<level>.ncp` in world space (with a lookup grid), `<model>.ncp` in model space for the `.prm` of the same name. Checked: all 307 files except `models/dragoncock.ncp` (34 polygons announced, room for 26 - broken). 5 instance files are empty (0 bytes, no collision): `market1/fabric_01`, `nhood1/tree1`, `ship1/startline`, `ship2/startline`, `wild_west2/Pennant`.

```
u16       polygon_count
Polygon   polygons[polygon_count]      112 bytes each
Grid      grid                         only in the level file (21 files)
```

### Polygon (112 bytes)

```
i32       flags
i32       surface                      index into the surface table
Plane     plane                        the polygon plane
Plane     edges[4]                     one per side, normals point outward - a triangle uses 3
f32       xlo, xhi, ylo, yhi, zlo, zhi bounding box
```

`Plane` = `vec3 normal`, `f32 d`, with `normal · p + d = 0` (like `.rim`).

Corners: corner i = `plane ∩ edges[i] ∩ edges[i+1]` (mod 3 or 4). Checked against the bounding box over all files: 59,513 of 61,394 polygons within 0.1 units, 99.75 % within 5; the rest are non-planar quads whose corners leave the box (up to 216 units in `wild_west2.ncp`) - clamping to the box fixes them. 6 polygons have two parallel planes (no corner).

### Flags

From the Re-Volt Blender add-on, counts over all files:

| Bit | Name | Polygons | Note |
|---|---|---|---|
| 1 | quad | 40,068 | else a triangle |
| 2 | double | 941 | double sided |
| 4 | object only | 1,713 | cars and objects, not the camera |
| 8 | camera only | 16 | market1, muse1, bollard |
| 16 | non planar | 0 | |
| 32 | no skid | 21 | toylite, wild_west2 `Mine_ramp` |
| 64 | oil | 0 | |
| 1024 | unknown | 327 | muse2 models, nhood2 `bin` - unchecked |
| 2048 | unknown | 189 | market1/2 `forklift`, `post`, `poster` - unchecked |

### Grid

```
f32       x0, z0                       corner of the grid
f32       x_count, z_count             cells (stored as floats)
f32       raster                       cell size
Cell      cells[z_count][x_count]      i32 count, i32 polygon[count]
```

Speeds up the lookup in the original. Not needed with rapier.

### Surfaces

The material table of `revolt.exe`: 27 records of 40 bytes, found by the ICE2 values (0.45, 0.45, 0.2). Names from the Blender add-on (table order). Roughness, grip and hardness match RVGL `properties_default.txt`; the conveyor velocities do not (RVGL: `0 -500 0` / `0 500 0`). conveyor1 / conveyor2 are the two escalators of muse2 (22 sloped polygons each, a stair down along +x / -z); their velocities (500 units/s) point along them, but the vertical part has the opposite sign of the slope - projected onto the polygon plane, conveyor1 runs up, conveyor2 down. How the exe applies them to the wheels is not decoded (the remake: the tires grip relative to the moving surface).

```
u32       flags                        4 out of bounds, 8 corrugated, 16 moves, 32 dusty; 1 and 2 unclear (spark / skid?)
f32       roughness
f32       grip                         scales the tire friction
f32       hardness
u32       unknown                      a small float or a packed value, 0 for boundary and conveyors
u32       corrugation                  type: 1 pebbles, 2 gravel, 3 steel, 5–7 dirt
u32       dust                         type
vec3      velocity                     conveyors, units per second (direction not checked)
```

| Id | Name | Flags | Roughness | Grip | Hardness | Corrugation | Dust | Velocity |
|---|---|---|---|---|---|---|---|---|
| 0 | default | 3 | 1.0 | 1.0 | 1.0 | | | |
| 1 | marble | 3 | 0.9 | 0.9 | 0.5 | | | |
| 2 | stone | 3 | 0.9 | 0.9 | 0.5 | | | |
| 3 | wood | 2 | 0.8 | 0.8 | 0.3 | | | |
| 4 | sand | 34 | 0.5 | 0.6 | 0.0 | | 2 | |
| 5 | plastic | 2 | 0.7 | 0.9 | 0.2 | | | |
| 6 | carpettile | 2 | 1.0 | 0.7 | 0.0 | | | |
| 7 | carpetshag | 2 | 1.0 | 0.7 | 0.0 | | | |
| 8 | boundary | 4 | 1.0 | 1.0 | 0.0 | | | |
| 9 | glass | 3 | 0.7 | 0.8 | 0.3 | | | |
| 10 | ice1 | 3 | 0.4 | 0.4 | 0.2 | | | |
| 11 | metal | 3 | 0.8 | 0.8 | 0.4 | | | |
| 12 | grass | 42 | 0.7 | 0.5 | 0.0 | 3 | 3 | |
| 13 | bumpmetal | 11 | 0.8 | 0.8 | 0.4 | 3 | | |
| 14 | pebbles | 43 | 0.7 | 0.7 | 0.2 | 1 | 1 | |
| 15 | gravel | 43 | 0.9 | 0.8 | 0.2 | 2 | 1 | |
| 16 | conveyor1 | 24 | 1.0 | 1.0 | 0.0 | 3 | | -287.4 128.7 388.4 |
| 17 | conveyor2 | 24 | 1.0 | 1.0 | 0.0 | 3 | | 287.4 128.7 -388.4 |
| 18–20 | dirt1–3 | 42 | 1.0 | 1.0 | 0.0 | 5–7 | 4 | |
| 21 | ice2 | 3 | 0.45 | 0.45 | 0.2 | | | |
| 22 | ice3 | 3 | 0.5 | 0.5 | 0.2 | | | |
| 23 | wood2 | 2 | 0.8 | 0.8 | 0.4 | | | |
| 24 | conveyor_market1 | 24 | 1.0 | 1.0 | 0.0 | 3 | | 0 0 357.8 |
| 25 | conveyor_market2 | 24 | 1.0 | 1.0 | 0.0 | 3 | | -357.8 0 0 |
| 26 | paving | 3 | 0.9 | 0.9 | 0.5 | | 5 | |

After the table follows the corrugation table (amplitude, wavelength x, z: pebbles 3 / 70 / 70, gravel 1 / 40 / 40, ...).

The masses of the physics objects are not in a data table: `revolt.exe` has a model name table (87 pointers to `modelsarrel`, `modelseachball`, ... `models\spheadgarden`, no extension - index = model id) with nothing around it, and the gravity 2200.0 exists once as a global constant. So the values are immediates in the init code per type - only disassembly would find them.

Polygons per surface over all files: default 50,234, grass 3,177, stone 1,380, wood 1,254, marble 1,221, carpetshag 1,091, carpettile 722, gravel 613, dirt3 374, dirt2 331, glass 239, metal 232, pebbles 145, dirt1 125, bumpmetal 96, ice1 92, conveyor1 22, conveyor2 22, ice3 8, ice2 6, conveyor_market1 4, boundary 3, conveyor_market2 3. Never used: sand, plastic, wood2, paving.

## .hul

Collision of cars (`cars/<car>/*.hul`, named by `COLL` in `Parameters.txt`) and physics objects (`models/<model>.hul` next to the `.m`). Layout from the Re-Volt Blender add-on, checked against all 44 files: 39 parse to the last byte. `cars/candy/Hull.hul` is empty (the car uses `Candy.hul`), `models/Bucket.hul` ends after the hulls (no sphere count), `cars/mouse/hull.hul` has 644 bytes left over, the second hull of `cars/panga/hull.hul` contains NaN.

```
i16         hull_count
ConvexHull  hulls[hull_count]
i16         sphere_count               missing in Bucket.hul
Sphere      spheres[sphere_count]
```

```
ConvexHull
i16       vertex_count, edge_count, face_count
f32       xlo, xhi, ylo, yhi, zlo, zhi bounding box around offset
vec3      offset                       center of the hull
vec3      vertices[vertex_count]       model space (absolute, not around offset)
i16       edges[edge_count][2]         vertex indices
Plane     faces[face_count]            normal, d - outward, inside: normal · p + d <= 0

Sphere
vec3      center
f32       radius
```

The faces have no corner list: a face is every vertex within 0.5 units of its plane (all hulls of the physics objects give complete faces). The spheres fill the same volume with balls (cars: 4–34, trolley 14 of radius 28) - what the game uses them for (world vs. object collision) is not checked.

Gravity: `Gravity 2200` in every `Parameters.txt` ("no longer used" for the body, the wheels still list it) and the global 2200.0 in the exe - 22 m/s² at 0.01 per unit, the converter writes it into the scenes.

Sizes at 0.01 per unit: the trolley hull is 2.1 × 2.1 × 3.2 m, a bottle 1.3 m tall, an ABC block 0.6 m - the world is about 2.2 times real size (the gravity of 2200 units/s² gives 9.81 m/s² at 0.0045 m per unit).

In the exe the gravity is a vector `(0, 2200, 0)` (y down), followed by bounding box constants (±1,000,000).

Speed unit: `TopEnd` ("actual top speed (mph)") is units/s, `TopSpeed` mph. `TopEnd / TopSpeed` lies at 86–92 for most stock cars (cars above the theoretical speed higher, wincar 1–3 / trolley / panga / ufo set by hand) - about 88 units/s per mph, so the speedometer counts 1 unit ≈ 5.1 mm (0.2 inch). With it the gravity is 11.2 m/s² (1.14 g). At 0.01 per unit the converted world keeps the time scale of the game: 22 m/s² is right, jumps take as long as in the game. No float 88 / 1/88 in the exe - the conversion is computed or a double elsewhere (not checked).

`Acc` ("acceleration rating (empirical)", 4.4-11 for stock cars, 50 for the hand made ones): read as m/s² of the speedometer - rc 6.86 ≈ 0.61 g at 1.14 g gravity. Not checked against the game; the remake drives with it (see HANDOVER).

Community docs (re-volt.io, Parameters Notes 7.1): wheel `Gravity` is "not literal gravity" (how fast the tire grips again after a landing), `MaxPos` the halved vertical travel, spring `Restitution` -1..0 the rest height on the shocks. Not checked against the exe.

## .taz

Track zones: oriented boxes the cars pass in id order. Checked (all levels, also `reversed/`).

```
i32 count
count × 64 bytes:
  i32  id          order on the track 0..n-1, several boxes can share an id (muse1: 51 boxes, 43 ids)
  vec3 center
  vec3 axes[3]     rows = local axes, orthonormal
  vec3 half_size   along the axes
```

- Battle / stunt levels and the frontend have a single zone.
- The loader sorts the boxes by id (stable bubble sort) and builds a table per id (box count, first box). Per box it keeps the three planes (`axis`, `-axis · center`) and the AABB of the 8 corners.
- Point inside: inside the AABB and `|axis · (p - center)| <= half_size` on every axis.
- Every box also holds the list of the AI nodes inside it, used to find the AI node of a car.

## .pan

Position nodes: a graph along the track with the distance to the finish line. Checked (all levels).

```
i32 count          0: the file ends here (battle, stunts, frontend)
i32 start node
f32 lap length
count × 48 bytes:
  vec3 position
  f32  distance    to the finish line, 0 at the start node, falls in driving direction
  i32  prev[4]     -1 = none
  i32  next[4]     in driving direction
```

Some nodes branch (up to 19 in market1). The loader also stores `2000 / lap length` (at least 0.1), use unknown.

## .fan

AI nodes: track cross sections with the racing line. Checked (all levels). The bots of milestone 4 need it, the reposition already uses it.

```
i16 count
i16 unknown        the exe allocates count + this many nodes
count × 76 bytes:
  u8   priority
  u8   start node
  u8   unknown flag
  u8   pad
  f32  racing line         0 = left edge, 1 = right edge
  f32  finish distance     like .pan, falls in driving direction
  f32  unknown 0..1        maybe an overtaking line
  i32  unknown             mostly 2
  i32  speed?, i32 speed?  mostly 30 (racing line / centre speed - unchecked)
  i32  prev[2]             -1 = none
  i32  next[2]             in driving direction
  i32  speed? (right edge), vec3 right edge
  i32  speed? (left edge),  vec3 left edge
if count > 0:
  i32  unknown node index
  f32  unknown length      about one lap
```

Left / right: at every node of every level the second edge lies right of the driving direction (glTF coordinates, towards the next node).

## .tri

Triggers: oriented boxes with a type. Checked (all levels, also `reversed/`). The game has a handler per type.

```
i32 count
count × 68 bytes:
  i32  type
  i32  flag        meaning by type
  vec3 center
  vec3 axes[3]     rows = local axes
  vec3 half_size   like .taz
```

| Type | Count (all files) | Meaning |
|---|---|---|
| 0 | 2 | the piano of toylite (and its reversed file) - see [Piano](#piano-game-behaviour) |
| 1 | 80 | lap / split (also called on a lap) - not decoded |
| 2 | 442 | track direction sign: flag = order << 16 \| sign |
| 3 | 14 | ? |
| 4 | 11 | ? |
| 5 | 27 | ? |
| 6 | 2 | ? |
| 7 | 0 | ? |
| 8 | 8 | ? |

**Track direction signs** (type 2):
- The player counts the signs of a lap (0 at the start and on every lap). The orders of a level are 0, 1, 2, ... in driving direction; several boxes can share one (forks).
- A trigger whose order equals the count increments it and shows its sign with a timer of 640. Sign 11 shows nothing.
- The HUD:
  - The timer runs down 720/s.
  - Shown at (288, 16, 64, 64) centered, the place of the wrong way X - not while wrong way shows.
  - Alpha `2 × (640 - t)` above 512, 255 between, `2 × t` below 128.
- Signs (fxpage2 texels, a negative width mirrors):

| Sign | Texels | Arrow |
|---|---|---|
| 0 | (127, 1, -62, 62) | S bend left (mirrored) |
| 1 | (63, 1, -62, 62) | U turn the other way (mirrored) |
| 2 | (126, 65, -61, 62) | 90° left (mirrored) |
| 3 | (255, 1, -62, 62) | curve left (mirrored) |
| 4 | (65, 1, 62, 62) | S bend right |
| 5 | (1, 1, 62, 62) | U turn |
| 6 | (65, 65, 62, 62) | 90° right |
| 7 | (193, 1, 62, 62) | curve right |
| 8 | (1, 65, 62, 62) | straight |
| 9 | (129, 1, 62, 62) | ! |
| 10 | (193, 65, 62, 62) | fork |

## Physics objects

Nothing in the `.fob` says an object is simulated - the type decides in code. Rigid bodies (seen in the game: bottles, packets, trolleys are pushed around):

| Type | Shape | Mass (game units, the object init of the exe sets mass and inverse mass) |
|---|---|---|
| 1 Beachball, 15 Football, 43 BasketBall | sphere (no `.hul`) | 0.1, 0.2, 0.3 |
| 7 Trolley | `cars/trolley/TrolleyHull.hul` + 4 wheels | 4.0 + 4 × 0.15 from its `Parameters.txt` |
| 57 Bottle, 58 Bucket, 59 Cone, 66 Packet, 67 ABC Block | `<model>.hul` | 1.2, 1.5, 1.6, 1.6, 0.8 |
| 36 Wobbly Cone | `cone.hul` | 0.5 |

The ABC block init also sets an inertia of 600 on the diagonal (inverse 1/600). For comparison: the body of `rc` has mass 2.0. Friction and restitution of the objects are not decoded.

The trolley is a car without drive in the original: `cars/trolley/Parameters.txt` (`Selectable TRUE`, `Obtain -1`) with `IsPowered FALSE` on the front wheels, 4 wheels `TrollWL.m` / `TrollWR.m`. Wheels at `Offset1` (see [Parameters.txt](#parameterstxt)). Placement (`.fob` position against the `.ncp` floor below): bottles, packets and ABC blocks sit within ±2 units on the floor, a shelf or the block below (stacks of 60 units). The trolleys hang 18–45 units above the floor (wheels at `Offset1` y 0) - they drop at the start. Not simulated (spin, move in code): 0 Spinning Barrel, 52 Tumbleweed, `firework.hul`, `Probe.hul`. The Wobbly Cone is a rigid body in the exe (outside mode 1 only) - the six of the frontend stand on the table of the options view, the clockwork cars knock them over (see [Frontend](#frontend-game-behaviour)).

## Parameters.txt

Car setup, text. `KEY values ;comment`, values split at whitespace and commas, strings in double quotes, blocks `BODY {`, `WHEEL n {`, `SPRING n {`, `PIN n {`, `AXLE n {`, `SPINNER {`, `AERIAL {`, `AI {` closed by `}`, all inside one outer `{ }`. A line of numbers only continues the key above (`Inertia`, 3 rows). `MODEL n "path"` lists the parts (0 body, 1–4 wheels, 5 spring, 9 axle, 13 pin, 17/18 aerial), `"NONE"` = no model. Paths mix the case of folders too (`cars\Adeon\car.bmp`, the folder is `adeon`). Stock cars (rc, candy, mouse) end with a checksum after the outer `}` (`EF33426` + a 0 byte).

Car polygons are drawn with the `TPAGE`, whatever page index they hold: rc 0, adeon 6, cougar / dino 8, moss 12, ufo / tc2 / tc4 15, q 16, rotor / wincar 17, beatall 19, amw 22, toyeca 23, tc1 15 and 3 (most likely the page slot the car had while it was built). The trolley has `TPAGE "NONE"` and page 24 (`levels\market1	rolley.bmp`).

Parts, as the converter places them (the game moves them every frame):
- **Wheel**: the model sits at `Offset1`. `Offset2` is already modelled into it: the tread center lies about `Offset2` aside of the model origin (rc front left: tread x -8.9..1.1, center -3.9, `Offset2` -4.5; cougar -2.0 / -1.5; trolley -5 / -4.5). So `Offset1 + Offset2` is the tread / collision center. A Korean RVGL wiki says the same (Offset1 = where the wheel is drawn).
- **Spring**: model along +y (rc: y 2–15, `Length` 16), from `Offset` towards the wheel.
- **Axle**: model along +z (rc: z 0–12, `Length` 12), from `Offset` towards the wheel.
- **Pin**: model along -y (rc: 15 long); placed on the wheel center + `Offset` - not checked.
- **Aerial**: `misc/Aerial.m` is one section of length 1 along -y, `Aerialt.m` the top; `Length`, `Direction`, `Stiffness`, `Damping` - the game bends it in sections (count unknown).
- **Spinner**: a model turning around `Axis` with `AngVel` (unit not checked): wincar 1–4 the wind-up key (`key.prm`, -0.5), sgt the radar (`radar.prm`, 1.0), panga the head (`head.prm`, 0 - moved some other way?). The other cars have `ModelNum -1`. The ufo has no wheel models at all (it hovers) and no spinner.

- **CoM** is relative to the center of the body bounds ("model centre", the body `Offset` is "calculated in game"), not to the origin: from the origin it lies below the wheel bottom for 10 cars (amw, fone, moss, mud, sgt, toyeca, wincar 1–3, trolley), from the body center it is above the ground and in the lower body for all 35 (trolley: low in the basket). Not checked against the game code.
- **Inertia** (BODY, 3 × 3, diagonal only): mass × units², × scale² gives kg m². rotor: wheels at ±41 units next to a body of ±18 - a box of the body gives a roll inertia 6 times too small and the springs make it jump; with the BODY Inertia all 33 cars with wheels settle calmly (simulated).
- **q** and **ufo** have no wheel models (all `ModelNum` of their wheels are NONE).

The meaning of the handling values (TopSpeed, EngineRate, Grip, StaticFriction, ...) is still open.

## Object motion (game behaviour)

Observed in the game code. Every `.fob` type has an init that loads its models (with an optional texture page offset) and installs a per frame, a move and a render handler.

Conventions:
- A matrix is 3 rows = the local axes (right, up, look), a point is `v.x * row0 + v.y * row1 + v.z * row2 + pos`. Turning `R` in the local frame of `W` is `R * W` in row major math.
- Angles are in turns. RotX / RotY / RotZ are the usual axis rotations; the combined rotation `Rot(a, b, c)` takes a and c in turns and b in turns the other way.
- `dt` is the frame time in seconds (the physics sub steps use 0.008 s). Some handlers use the clock in ms instead.
- Random numbers are the C runtime `rand()` (0..32767), `rand() * 3.05e-5` = 0..1.

| Type | Motion |
|---|---|
| 0 Spinning Barrel | spins about its local X: `flag0 / 32768 * 72` turns/s. No `.hul` / `.ncp` - it does not collide, the level `.ncp` carries the cars through the tunnel (muse2) |
| 2 Planet | orbits the planet whose `name` is its flag1 (none if it is its own name): on the first frame offset `(0, 0, distance to it)` in the frame `RotZ(0.1 turn) * L` (L: look axis from the parent to the planet, right axis horizontal), then that frame turns about its local X with `|flag2| / 32768 * 72` turns/s; the planet spins about its local Y with `flag3 / 4096 * 72` turns/s. Moons follow the moving parent. Planet 11 (the sun) has no model, it makes a light and a star field |
| 3 Plane | circles around its start: `pos = start + RotY(a) * (0, 0, flag1)`, `a += dt * flag0 / 16384 * 72` turns; orientation `bank * RotY(a)` (row major, bank in the local frame) with bank = `RotZ(flag2 / 512 turn) * RotY(0.25 turn, 0.75 if the speed is not positive)` - its own rotation in the .fob is not used. Propeller `plane2.m` at `(0, -120, 110)`, 2 turns/s about Z |
| 4 Copter | flies around in a box: x `start ± flag0 * 10`, y `start - flag3 * 50 ± flag1 * 10`, z `start ± flag2 * 10`. Waits 2 s, then states: pick a random point in the box at least 1000 units away (10 tries), 80 % of the time turn to face it horizontally (else keep the heading); fly: speed += dt * 100 up to 300 units/s, capped to the distance left, the heading slerps in 5 s with `(sin((t - 2.5) * 0.2π) + 1) / 2`, a tilt of `speed / 300 * -0.15` about up × direction; within 10 units: next point. Rotors: `copter2.m` at `(0, -340, 66)` 2 turns/s about Y, `copter3.m` at `(28, -252, -200)` 2.5 turns/s about X. Sound `toy/copter` |
| 5 Dragon | `dragon1.m` body, `dragon2.m` head morphs to `dragon3.m` (same vertices, the positions are blended). Cycle of 8 s, weight towards dragon3: 0-2 s `t / 2`, 2-2.2 s `1 + 0.03 sin((t - 2) * 5π)`, to 4 s 1, 4-6 s `(6 - t) / 2`, 6-6.2 s `-0.03 sin((t - 6) * 5π)`, then 0. Fire particles while open, head offset `(-494.2, 208, 96)` |
| 8 Boat | bobs `y = start + 15 sin(2π t / T2)`, rocks `Rot(sin(2π t / T1) / 90, sin(2π t / T3) / 90, 0)` in its local frame; T1, T3 = 4 + 0..2 s, T2 = 2 + 0..1 s, random per boat |
| 10 Radar | turns about world Y, 0.75 turns/s (its .fob rotation is replaced), two light beams |
| 11 Balloon | `y = start + 16 sin(t + random phase)` |
| 12 Horse | rocks about its local X: `sin(4 t + random phase) * 0.02` turns (±7.2°, 1.57 s), `toy/creak` at the turning points |
| 13 Train | drives along world -z at 200 units/s; below z = -11500 it jumps to z = -400 (toy2). Wheels at the fixed offsets `(±98, -136, 330)` (`train2.m`, -0.3 turns/s) and `(±98, -92, -204)` (`train3.m`, -0.5 turns/s) about their local X. Smoke at `(0, -550, -196)` every 0.1 s, whistle when a car is near |
| 17 Space Man | turned by `Rot(0.001, 0.001, 0)` every frame - a tumble that depends on the frame rate |
| 40 Sprinkler | head (`sphead.m`, garden `spheadgarden.m`) at `(0, -38, 0)` turns about its local Y at -0.2 turns/s plus a ratchet `sin(p) * 0.04` turns, `p += dt * 14.285`; a car on its hose (41) stops it for a moment; water particles |
| 56 Slider | moves `±400` units along its local X: triangle wave from the clock, `s = (ms mod 3000) / 1500`, `2 - s` above 1; flag0 picks the sign. Sounds `market/sdrsopen` / `sdrsclos` |
| 71 Dolphin | no model - every few seconds a jumping dolphin sprite |

Sounds of the moving objects (entries of the level sound block, so the file depends on the level): plane and copter loops (toy/plane, toy/copter), train loop (toy/train) and toy/whistle once a car is within `|dx| < 400`, `-400 < train.z - car.z < 800`; dragon (toy/dragon) once when the mouth opens (at 2 s); horse (toy/creak) when `sin(phase)` passes `±0.01` (22050 Hz one way, 20000 Hz the other); sprinkler (hood/sprink - in garden1 that block entry is garden/animal3) on every ratchet peak, volume `on * 127`; slider market/sdrsclos at the open end, sdrsopen at the closed end. All with range 1.0 (full volume up to 6 m).

Types with physics instead of a scripted motion: 36 Wobbly Cone, 52 Tumbleweed (blown by wind). 70 Gari flag is cloth (its own vertex simulation). Strobes, lasers, sparks, rain, lightning and ship lights are effects.

## Camera (game behaviour)

A table of follow cameras: two vectors in the car frame, game units - the camera position and the point it looks at. Entry 0 is set when a race starts:

| Entry | Position | Looks at |
|---|---|---|
| 0 (default) | (0, -150, -460) | (0, -60, 0) |
| 1 | (0, -120, -210) | (0, -70, 0) |
| 2 | (-310, -95, 0) | (0, -20, 15) |
| 3 | (310, -95, 0) | (0, -20, 15) |
| 4 | (0, -90, 210) | (0, -30, 0) |
| 5 | (0, -120, -280) | (0, -60, 0) |

The same for every car. Entry 0: 4.6 m behind, 1.5 m up, looking at 0.6 m above the car - 4.7 m from the look point at 11° down. Which key picks which entry is not decoded.

## Race setup (game behaviour)

**Number of cars** (menu "Number of Cars", setting `NCars`): 2-12, default 8. It counts all cars of the race, local players included - the CPU cars fill up to it. Laps (`NLaps`): 1-20. Some builds (demo-like modes) cap both at 8.

**Car ids**: a fixed list in the game code, not the folder order:

| Id | Cars |
|---|---|
| 0-7 | rc, mite, phat, moss, mud, beatall, volken, tc6 |
| 8-15 | dino, candy, gencar, tc4, mouse, flag, tc2, r5 |
| 16-23 | tc5, sgt, tc3, adeon, fone, tc1, rotor, cougar |
| 24-31 | sugo, toyeca, amw, panga, trolley, wincar, wincar2, wincar3 |
| 32-34 | wincar4, ufo, q |

**Opponents** (single race, when random cars is off) are picked by the `Rating` of the first player's car:
- Candidates: every car except the player's own and rotor, panga, trolley, the four wincars, ufo, q. Grouped by `Rating` (0 rookie .. 4 pro).
- Starting at the player's rating and going down, each rating gives as many cars as it has until the field is full (`cars - 1` opponents); the last rating used gives only what is still missing. Higher ratings are never used.
- Within a rating the cars are random, no duplicates.
- If all lower ratings together are still too few (a rookie car with 11 opponents), the list is filled up by repeating the picked cars from the start.
- Candidates per rating: rookie rc, mite, phat, moss, mud, beatall, volken, tc6 (8); amateur dino, candy, gencar, tc4, mouse (5); advanced flag, tc2, r5, tc5, sgt (5); semi-pro tc3, adeon, fone, tc1 (4); pro cougar, sugo, toyeca, amw (4).
- Examples: rc (rookie), 8 cars: the other 7 rookies. amw (pro), 8 cars: cougar, sugo, toyeca + 4 semi-pro.
- Random cars on (unchecked): one random selectable car, apparently used for every car of the race.

## Race progress (game behaviour)

Every frame, for every player, the zone update runs, then the progress.

**Zone** (0 at the start):
- The car is in a box of its zone z: nothing changes.
- Else in a box of z + 1 (mod zone count): z + 1. Else in a box of z - 1: z - 1. Else z stays.
- One step per frame, so a skipped zone (a shortcut) leaves z behind and the progress below stops until the car is back in z.

**Progress**, only while the car is in a box of z. Outside of it only the wrong way flag is set.
- Current `.pan` node n (the start node at the start): the nearest of n, its prev and its next nodes (squared distance). One step per frame.
- Direction `d = normalize(Σ normalize(n - m))` over the next nodes m. It points against the driving direction.
- Distance to the finish `r = n.distance + d · (car - n)`.
- Wrong way: `d · car look > 0.6`. The HUD only switches after the flag held for 1 s.
- Lap fraction: follows `r / lap length` by the wrapped difference (±0.5), wrapped to 0..1.
- Finish line: old fraction < 0.25 and new > 0.75 is a crossing forwards. Old > 0.75 and new < 0.25 is a crossing backwards and sets a backwards flag.
  - A forward crossing with the backwards flag set only clears it and counts no lap.
  - At the start the backwards flag and a "not crossed yet" flag are both set. The cars stand behind the line, so their first crossing counts no lap and only starts the clock.
- A lap also gives the time since the crossing inside the frame: `(1 - new) / (old + 1 - new) × dt`.
- Local players only (unchecked): a smoothed angle (3 rad/s) between the car heading and the track direction. The track direction comes from a spline through the averaged prev / next nodes at the fraction ± 0.1. Probably the direction arrow of the HUD.

**Laps**:
- While "not crossed yet" is set, the lap start is "now", so the lap clock starts at the first crossing.
- On a lap: lap time = now - lap start - crossing time; laps + 1; last lap; best lap = min; the new lap start is now - crossing time.
- At laps == the lap count of the race the car has finished. The race time comes from the global race clock.

**Race position**: `score = (laps - backwards flag) × lap length - r`, as an int. The highest score leads, the places are sorted by it.

**Reposition** (the car is put back on the track) uses the AI nodes, not the `.pan`:
- The AI node of the car is updated every frame. It is the node whose quad (its left / right edge to the left / right edge of its next node) contains the car in x / z.
  - The search starts at the last node and follows its links up to 2000 units away.
  - Then it tries the AI node lists of the boxes of the car's zone and of the zones ± 1.
  - This zone is a second one, without the order rule: the box the car is in.
- Nodes whose next node has priority 6 are skipped, the next one is taken.
- Position: on the racing line (`lerp(left, right, racing line)`) of the node, 100 units towards the racing line point of its next node, then 100 units up. Orientation along that line (unchecked).
- In one game mode (unchecked which) the car is put on a start position instead, and zone, `.pan` node and the flags are reset like at the start.
- Automatic reposition (unchecked): CPU cars get the request when they are outside of every zone for 1 s, without an AI node for 10 s, or moved less than 2 m in 5 s; the reposition starts after 3 s of a condition.

## HUD (game behaviour)

Drawn every frame on a 640 × 480 screen, scaled. Right aligned and centered items keep their distance to the right edge / the center (0 extra width at 4:3).

Draw primitives:
- **Text** (x, y, char w, char h, color ARGB, text).
  - Glyphs come from `gfx/font.bmp`, glyph = char - `!`, 32 per row.
  - Texels per glyph: `(col × 8 + 0.5, row × 16 + 1)`, 7 × 16.
  - Each char is drawn w × h and advances by w. Chars above 127 are mapped to accented glyphs.
- **Sprite** (x, y, w, h, u, v, uw, vh, color): uv in 0..1 of the current page.

Colors: labels cyan `0xFF00FFFF`, values white. The labels come from `strings/<language>.txt` (Best Lap, Last Lap, Lap, Race, Lap: english.txt lines 311-315; st / nd / rd / th).

| Item | Position | Content |
|---|---|---|
| "Lap" + laps | (16, 16) / (16, 32), 8 × 16 | `%d/%d` = laps + 1 / race laps |
| weapon frame | (16, 56, 64, 64) | fxpage2 (190, 157, 64 × 64), `0xE0FFFFFF`, only with weapons on |
| weapon icon | (32, 72, 32, 32) | fxpage2, see the weapon table |
| wrong way | (288, 16, 64, 64) centered | fxpage2 red X (129, 65, 62 × 62), `0xC0FFFFFF`, while bit 8 of the clock in ms is set (256 ms blink) |
| Last Lap / Best Lap / Lap / Race | labels right aligned to x 624 (`(78 - len) × 8`), y 16 / 56 / 96 / 136; values x 552, y + 16 | `%02d:%02d:%03d` of last, best, current lap and the race clock |
| position | (16, 392), 12 × 24 | `%d`; suffix 8 × 12 cyan at x 40 (below 10) else 28 |
| direction arrow | center (96, 396) | fxpage2 up arrow (2, 66, 60 × 60) on a square with half diagonal 32, turned by the arrow angle, `0xC0FFFFFF` |
| fps | (144, 392) / (144, 404), 8 × 12 | only with the fps option |
| ahead / behind | (16, 432) `-%ldm` + name (72, 432); (16, 448) `+%ldm` + name (72, 448) | nearest car ahead / behind by `laps - fraction - backwards flag`. Distance = difference × lap length × 0.005 (m of the game). Color by `v = min(m × 10.24, 511)`: ahead green → yellow → red, behind red → yellow → green |
| speed | value `%4d` (560, 448), unit (600, 448), 8 × 16 | speed (units/s) × factor: mph 0.01118 (1 mph = 89.4 units/s), kph 0.018 |
| speed dots | 7, see below | the dark dot, the lit one 25 texels lower; sprite size = texel size |

Speed dot positions (right aligned) and dark dot texels on fxpage2 (u, v, w, h):

| Dot | Position | Texels |
|---|---|---|
| 0 | (492, 451) | (143, 128, 15, 14) |
| 1 | (502, 435) | (119, 128, 16, 16) |
| 2 | (517, 422) | (95, 128, 19, 18) |
| 3 | (536, 412) | (71, 128, 19, 19) |
| 4 | (558, 405) | (47, 128, 21, 21) |
| 5 | (581, 400) | (23, 128, 22, 22) |
| 6 | (606, 399) | (0, 128, 22, 24) |

**Speed dots**:
- steps = 8 × wheel speed / top speed. The wheel speed is the mean of angular velocity × radius over the powered wheels.
- Each dot has a fill 0..1024: below `floor(steps)` 1024, at it the fraction, above 0.
- The fill rises 4096/s and falls 1024/s.
- The dark dot is drawn with alpha `0xC0 - lit`, the lit one with `lit = fill × 0xC0 / 1024`.

Not decoded:
- the finish display after the race (big digits of font.bmp, results table);
- the split time message (`+%02d:%02d:%03d` at y 128 / 144);
- modes 6 / 8 / 9 (battle, stunt);
- which unit is the default.

## Countdown (game behaviour)

- **Timer:** 5 s at the race start.
- **Beeps:** below 3.5 s, every time the full second drops, `countdown` plays at 22050 Hz, volume 127. That makes beeps at 3, 2 and 1.
- **GO:** when the countdown runs out, `countdown` plays twice at once, at 31000 Hz and 37000 Hz.
  - The race clock starts with the time left over.
  - Until then the lap / race clocks stand still.
  - Zone and `.pan` progress are updated during the countdown too, without laps.
- **Models:** `models/go3.m`, `go2.m`, `go1.m` and `gogo.m`.
  - They have no texture and only vertex colors, red / yellow, about 50 units high.
- **Display:** visible while the countdown is below 3 s, and until the race clock passes 1 s.
  - The time `t` (ms) is countdown + 1000, after GO 1000 - race clock. The model is `t / 1000`: 3 go3, 2 go2, 1 go1, 0 gogo.
  - Drawn by a camera at (0, -32, z) game units with its own view, lens 640 on the 640 × 480 screen (unchecked).
  - Turn: RotY of `(t mod 1000 - 100) × 0.0025` turns in the first 100 ms of its second and `(t mod 1000 - 900) × 0.0025` in the last 100 ms - a flip of 90° in and out.
  - No turn above 3500 ms and below 500 ms (the 3 does not flip in, GO does not flip out).
  - z is 256. Above 3800 ms it is `256 - (t - 3800)`: the 3 comes from close. Below 200 ms it is `256 - (200 - t)`: GO flies at the camera.

## Pickups and weapons (game behaviour)

**Pickups**: every `.fob` pickup (type 30) takes a slot of a fixed pickup array (40 slots; level pickups and dropped ones). Each slot has a state:

| State | Meaning |
|---|---|
| gone | |
| waiting | a timer counts 5 s down, then out and the 3D sound `pickgen` at the pickup |
| out | the timer counts up. It grows from scale 0 (see below) and checks the cars |
| collected | rises with (half the car velocity x / z, 64 units/s up) and grows by 1 + 2t for 1 s |
| free | unused slot |

- **Growing while out:**
  - below 0.5 s: 0;
  - to 1 s: `2 (t - 0.5) + 0.6667 sin(2π (t - 0.5))`;
  - to 1.35 s: `1 - 0.1667 sin((t - 1) × 8.976)`;
  - to 1.6 s: `1 + 0.0833 sin((t - 1.35) × 4π)`.
- **Spin:** all pickups spin together, 0.5 turns/s about Y.
- **Collect:** a player whose box overlaps the pickup box, only without a weapon and without a running spin.
  - The 3D sound `pickup` plays.
  - At the end of collected the pickup is gone, and a random gone pickup starts waiting.
- **At the race start** 2 × players + 5 random pickups start waiting, so they come out after the 5 s countdown.

**Weapon choice** (on collect):
- Human players use the row of their race position, others a fixed row (10 10 5 10 10 5 15 15 10 10 0).
- Pick: a random 0..32767 against the running sums (scaled to 32768). The first weapon with a weight whose sum is above the roll wins.
- Rows are weights in percent:

| Row | Weights |
|---|---|
| first place | 5 18 5 10 5 18 18 4 8 9 0 |
| middle | 10 × 10, 0 |
| last place | 20 3 15 2 15 2 2 21 13 2 5 |

- A row per position i of n players: f = i / (n - 1). The first half interpolates first → middle by f, the second half middle → last by f. With one player no row is built.
- **Spin:** after the pick a 4 s spin runs down, the shown icon is `(weapon + 22) - t² × 0.34375` mod 11, slowing down onto the weapon. At 0 the player gets it.
- **Ammo:** weapons 2 and 4 get 3, the others 1.

Weapons by index (icon texels on fxpage2, 32 × 32):

| Index | Weapon | Icon |
|---|---|---|
| 0 | Shockwave | (0, 224) |
| 1 | Firework | (64, 225) |
| 2 | Firework Pack | (96, 224) |
| 3 | Putty Bomb | (32, 224) |
| 4 | Water Balloons | (128, 224) |
| 5 | Electro Pulse | (224, 224) |
| 6 | Oil Slick | (192, 224) |
| 7 | Ball Bearing | (0, 192) |
| 8 | Turbo Battery | (32, 192) |
| 9 | Clone Pickup (name unchecked) | (160, 224) |
| 10 | Global Pulse (the star) | (96, 192) |

**HUD**:
- The icon is drawn additive (ONE / ONE) at (32, 72, 32, 32).
- While it spins: icon int(shown) in gray `0x80 - f × 0x80` and the next one in gray `f × 0x80`.
- Afterwards the icon is full white.
- Firework pack / water balloons show the ammo as `%d` at (60, 102), 6 × 8, `0xFFFFAA22`.

## Sounds and music (game behaviour)

**Global sounds**: 43 `wavs\...` files loaded at start, the index is the sound id.

| Ids | Sounds |
|---|---|
| 0-3 | engines: moto, petrol, clockwrk, ufo |
| 4-8 | honkgood, scrape, skid_normal, skid_normal (twice), skid_rough |
| 9-18 | pickup, pickgen, shock, shockfire, electro, electrozap, firefire, firebang, balldrop, ball |
| 19-28 | hit1, hit2, wbomb, wbombfire, wbombhit, wbombbounce, puttbang, fuse, oildrop, countdown |
| 29-42 | turbo, servo, menuNext, menuPrev, menuUpDown, menuLeftRight, beachball, lightflk, bottle, boxslide, starfire, tvstatic, splash, honka |

Which car plays which engine (0-3) is not found yet - the remake goes by `Class` (0 moto, 1 petrol), wincar* clockwrk, ufo ufo.

**Level sound blocks**: after the global sounds the block of the level is loaded, so its sounds get ids 43, 44, ... Blocks (in this order):

| Block | Levels | Sounds |
|---|---|---|
| toy | ToyLite, Toy2 | piano, plane, copter, dragon, creak, train, whistle, arcade, toybrick |
| hood | NHood1, NHood2, stunts, Nhood1_Battle | basketball, birds1, birds2, birds3, dogbark, kids, sprink, tv, lawnmower, digger, stream, cityamb2, roadcone |
| garden | Garden1, Bot_Bat | tropics2, tropics3, tropics4, stream, animal1-4 |
| muse | Muse1, Muse2, Muse_Bat | museumam, laserhum, alarm2, escalate, rotating, largdoor |
| market | Market1, Market2, MarkAr | aircond1, cabnhum2, carpark, freezer1, iceyarea, sdrsopen, sdrsclos, carton |
| ghost | Wild_West1, Wild_West2 | coyote1, bats, eagle1, minedrip, rattler, townbell, tumbweed |
| ship | Ship1, Ship2 | intamb1, seagulls, shiphorn, strmrain, thunder1, wash |

**3D Sound objects** (`.fob` type 49, flags name / range / random play):
- `name` -> a sound id: `44 47 48 50 51 52 45 46 50 53 43 44 45 43 43 44 45 46 47 46 47 48 46 43 44 45 46 47 43 44 45 47 46 48 47 48 49 50 54 48 44 ...` - minus 43 = index into the block of the level. So the file depends on the level the object is in.
- `random play` 0: a looped 3D sound (volume 127, 22050 Hz). Not 0: first wait `rand() * 0.00061` (0-20 s), then a one shot; after it ended a pause of `10 + rand() * 0.00061` (10-30 s), and so on.
- `range` × 0.1 is the range factor of the 3D sound: Miles distances `min = range × 3`, `max = range × 34`, range 0 means 1. Miles positions are game units × 0.005 (listener and sounds), so one Miles unit = 200 units = 2 m: min = range × 0.6 m, max = range × 6.8 m (range 10: 6 m / 68 m). Beyond max the voice is released, it starts again when the listener comes back in range. The cars create their sounds with range factor 1.0 (6 m / 68 m).
- Labels of the built-in editor by `name`, and the file in the level block of that kind. Checked on garden1 (10-12 tropics, 22 stream, 34-37 animals), market1 (14 air cond, 15 cabinet hum, 17 freezer), toylite (8 arcade):

| name | label | file | name | label | file |
|---|---|---|---|---|---|
| 0 | Hood Birds 1 | hood/birds1 | 20 | Muse barrel | muse/rotating |
| 1 | Hood Dog Bark | hood/dogbark | 21 | Muse door | muse/largdoor |
| 2 | Hood Kids | hood/kids | 22 | Garden stream | garden/stream |
| 3 | Hood TV | hood/tv | 23-27 | Ghost coyote, bats, eagle, drip, rattler | ghost/coyote1, bats, eagle1, minedrip, rattler |
| 4 | Hood Lawnmower | hood/lawnmower | 28 | Ship internal ambience | ship/intamb1 |
| 5 | Hood Digger | hood/digger | 29 | Ship seagulls | ship/seagulls |
| 6 | Hood Birds 2 | hood/birds2 | 30 | Ship foghorn | ship/shiphorn |
| 7 | Hood Birds 3 | hood/birds3 | 31 | Ship thunder | ship/thunder1 |
| 8 | Toy Arcade | toy/arcade | 32 | Ship storm | ship/strmrain |
| 9 | Hood Stream | hood/stream | 33 | Ship calm | ship/wash |
| 10-12 | Garden tropics2-4 | garden/tropics2-4 | 34-37 | Garden animal wierd, bird1, bird2, frog | garden/animal1-4 (in this order) |
| 13 | Muse ambience | muse/museumam | 38 | Hood ambience | hood/cityamb2 |
| 14 | Market air cond | market/aircond1 | 39 | Ghost bell | ghost/townbell |
| 15 | Market cabinet hum | market/cabnhum2 | 40-47 | Roof traffic, chopper, siren, wind, steam hiss, steam pop, telemetry, aircon | id 44 = index 1 of any block (cut Rooftops level) |
| 16 | Market car park | market/carpark | | | |
| 17 | Market freezer | market/freezer1 | | | |
| 18 | Market icy | market/iceyarea | | | |
| 19 | Muse escalator | muse/escalate | | | |

**Music**: cd audio, the Steam version maps the cd tracks to `music/TrackNN.ogg`. The audio tracks 2..15 (at most 14) form a list, index 0 = cd track 2. `REDBOOK a b` of the `.inf` are indices into it: a random track of a..b is played on to the end of b; when it stopped the same range plays again from the picked track. So `REDBOOK 1 3` = Track03-05, `4 9` = Track06-11, `10 10` = Track12, frontend `0 0` = Track02. Track13 / Track14 play once somewhere - probably results / win music, the condition is not decoded. Settings `MusicOn`, `MusicVol`.

## .lit

Lights of a level. Checked: frontend (2 lights); the other levels parse to the last byte.

```
i32       light_count
Light     lights[light_count]

Light (72 bytes):
vec3      position
f32       reach                      game units
vec3      right, up, look            a spot shines along look
f32       cone                       degrees
vec3      color                      0..255 and more (the frontend: 350)
u8        flag                       1 world, 2 kept while racing, 4 moving models (unchecked), 16 not into the models
u8        type                       0 omni, 1 omni normal, 2 spot, 3 spot normal
u8        speed
u8        pad
```

The game adds the lights to the corner colors at load (world: flag 1; instances: every light without flag 16), per corner and light:

- `d = light - vertex`; nothing beyond `reach`; `a = 1 - |d|² / reach²`
- normal types: nothing if `d · n <= 0`, else `a *= d · n / |d|`
- spots: `c = (-(d · look) / |d| - 1) * 180 / cone + 1`, nothing if `c <= 0`, else `a *= c`
- each channel `+= color * round(a * 256) / 256`, clamped to 0..255

The converter bakes them into the world, every lit instance (a model of its own) and the objects that do not move (also unchecked: the game lights objects at runtime).

## Frontend (game behaviour)

The menu runs in the `frontend` level (a toy shop). `FRONTEND.cam` is empty - the views are a fixed table in the exe. Checked with the converted level (2026-10-12): views 1, 2, 3, 4, 12 and 14 look at what the menus need.

### Views

24 entries: camera position and the point it looks at (game units, Y down).

| View | Position | Looks at | Used by |
|---|---|---|---|
| 0 | (76.5, -79.3, -4462.6) | (109.5, -155.3, -3466.1) | title (far back in the aisle) |
| 1 | (93.5, -102.9, -1926.7) | (110.9, -296.6, -945.7) | main menu, in-game menu (the counter) |
| 2 | (863.5, -1092.5, 512.5) | (1491.2, -561.9, 1082.2) | select car (the stack of car boxes) |
| 3 | (1355.5, -166.2, 864.4) | (2204.6, 360.0, 911.5) | the chosen car (the floor by the boxes) |
| 4 | (1589.6, -81.8, -1743.8) | (2445.7, -365.5, -1311.8) | select track (the big screen) |
| 5 | (-212.5, -664.6, -706.8) | (-212.5, -901.3, 134.4) | |
| 6-9 | (-1773.9, -503.9, -477.2) ... | (-2657.6, -41.1, -546.7) ... | bronze / silver / gold / platinum cup |
| 10 | (-1171.6, -137.8, 834.4) | (-2080.1, -287.1, 444.1) | progress table |
| 11 | (-816.2, -534.5, -500.0) | (-1591.0, -392.4, -1115.9) | options |
| 12 | (527.1, -930.5, 1322.3) | (1105.5, -375.5, 1922.1) | enter name (the name stand) |
| 13 | (-521.2, -2870.0, -325.4) | (-306.2, -1894.6, -276.3) | |
| 14 | (146.4, -919.2, -206.2) | (35.6, 33.0, -78.3) | select race (straight down onto the logo on the counter) |
| 15 | (-696.8, -493.2, -1728.4) | (-1611.2, -510.2, -2133.0) | best trial times |
| 16, 17 | (2233.9, -507.6, -1393.0), (2026.2, -407.2, -1501.5) | (3125.8, -664.1, -968.6), (2914.3, -579.5, -1075.5) | 17: summary |
| 18-23 | (-1047.1, -746.2, 1165.8) ... | (-1692.1, -377.2, 1835.1) ... | |

(6-9 and 18-23 in full: 7 (-1637.2, -764.6, 206.4) -> (-2420.9, -224.8, -101.0), 8 (-1855.2, -733.4, 669.9) -> (-2503.0, -83.4, 271.3), 9 (-2007.7, -713.5, 1026.9) -> (-2575.5, -29.3, 569.3), 19 = 18, 20 (-1436.8, -612.4, 1353.8) -> (-1906.3, -354.4, 2198.2), 21 (-1676.0, -534.9, 1268.1) -> (-1950.7, -21.6, 2081.1), 22 (-1052.3, -437.1, 1770.1) -> (-1926.2, -44.5, 2056.7), 23 (-1047.1, -746.2, 1165.8) -> (-1692.1, 0.0, 1835.1).)

- Every menu item names a view (or none - the camera stays). The camera follows the view of the selected item.
- A change of view is a flight: position lerp and rotation slerp with `s = sin(pi/2 * t / T)`, `T` 0.85 s (1.7 s in some intro case, 0.25 s in another).
- View 14 sits inside the hanging tv tower above the counter: the software renderer looks through its back faces. Our renderer does not - the app code hides the tower while that view is used.
- Lens: the game's default (640 on a 640 x 480 screen, about 41° vertical) - unchecked for the frontend.

### Menus

A menu: title string id, title icon, flags, position (640 x 480 space), then its items. An item: string id, view, handlers (left / right / up / down / select / back / draw), the next menu.

| Menu | Title icon | Position | Items (view) |
|---|---|---|---|
| main | - | - | Start Race, Best Trial Times, Progress Table, Options, Quit (1) |
| Select Race | 4 | (100, 150) | Championship, Single Race, Clockwork Carnage, Time Trial, Practice, Stunt Arena, Calculate Car Stats, Multi-Player, Edit Mode, Load Replay (14) |
| Enter Name | 8 | (100, 120) | Player (12) |
| Select Car | 1 | (100, 150) | the car (2), after the choice the same item views 3 |
| Select Track | 2 | (100, 380) | the track (4) |
| Select Cup | 3 | (100, 150) | Bronze / Silver / Gold / Platinum Cup (6 / 7 / 8 / 9) |
| Options | 7 | (100, 150) | Game / Video / Audio / Controller Settings, View Credits (11) |
| Best Trial Times | 0 | | (15) |
| Progress Table | 5 | | (10) |
| Multi-Player | 6 | | |

- Title: dark box at (108, 40), width `16 * length + 64`, height 32; text at (140, 44) in 16 x 24 chars. Icon: dark box (20, 20, 72, 72), icon (24, 24, 64, 64), white at half alpha. The icons are 48 x 48 texel cells on `frontendf.bmp`, 5 per row: 0 stop watch, 1 car, 2 track, 3 cup, 4 chequered flag, 5 progress, 6 multi-player, 7 computer (options), 8 person (name).
- Items: chars 8 x 12, one row per 16. Colors: normal white, selected `#ff7722`, not available `#444444`, selected and not available `#803010`. Dark box behind them: (x - 20, y - 24, width + 40, height + 44). The dark box is `#181818` at alpha `0xb0`.
- The main menu fades the logo of the game in (0.5 s).
- Ranges (Game Settings): Number of Cars 2-12 (default 8), Number of Laps 1-20.
- Sounds: up / down `menuUpDown`, left / right `menuLeftRight`, select `menuNext`, back `menuPrev` - all played at 44100 Hz (the wavs have 22050, so twice as fast, an octave up); `boxslide` when a car box comes out and `tvstatic` when the track picture changes at their own 22050 Hz.

### Car boxes (CarBox object, type 46)

- `flag0` = car id. The car ids are the order of the car table: 0 rc, 1 mite, 2 phat, 3 moss, 4 mud, 5 beatall, 6 volken, 7 tc6, 8 dino, 9 candy, 10 gencar, 11 tc4, 12 mouse, 13 flag, 14 tc2, 15 r5, 16 tc5, 17 sgt, 18 tc3, 19 adeon, 20 fone, 21 tc1, 22 rotor, 23 cougar, 24 sugo, 25 toyeca, 26 amw, 27 panga, 28 trolley, 29-32 wincar1-4, 33 ufo, 34 q. One box has -1 (no car).
- The frontend loads `carbox1-3.bmp` as pages 27-29 (over the fx pages). The first polygon of `carbox.m` with flag 0x800 is the picture: car 0-26 on page `27 + id / 9`, cell `(id % 3, (id % 9) / 3)` of 85 texels, 84 texels wide, 1 texel inset; car 27 page 4 (`frontende.bmp`) at (171, 73); -1 page 6 (`frontendg.bmp`, the "?" box) at (0, 171). Corner order (u + w, v), (u + w, v + w), (u, v + w), (u, v).
- A car that is not available draws `PlainBox.m` instead.
- States: home (back to its place: per axis `+= (target - p) * k * 50 dt` with k (0.2, 0.3, 0.2), rotation 0.85 share per 1/30 s; snaps within 1 unit or after 1.5 s), out (the selected car: in front of view 2 - 600 ahead, 150 right, 50 up -, facing the camera, k (0.2, 0.1, 0.2)), landing (chosen: on the floor 230 along view 3, turned to the camera), after 1 s the car comes out of it and the box flies home 0.3 s later; 1.3 s after that the menu goes on to the tracks. Boxes in flight push each other apart below 250 units.

### Name entry (type 39)

- `namestand.m`, the wheel `namewheel.m` at (0, -490, -80) of the stand, turning about its z axis.
- 30 positions: A-Z, space, !, DEL, END. The wheel turns to `letter / 30` turns, 1/6 of the remaining way per frame. Left / right step the letter (wrapping), select takes it; DEL deletes, END finishes; 14 chars at most (then END is forced).
- Text: "Player n" in orange, the name in yellow after it, a blinking `_` (500 ms).

### Clockwork cars of the options view

- The frontend adds three racers before the level loads: cars 29, 30, 31 (`wincar`, `wincar2`, `wincar3`, "Clockwork" cars), names `Dinky1` / `Dinky2` / `Dinky3`, player type 6 (as a local player, no camera) with control type 6. Not in the results frontend (that one adds the podium cars).
- Their start: `STARTGRID 2` of `Frontend.inf` (STARTPOS 0 0 0). Grid 2 of the exe's grid table, slots 0-2: (-1600, -250, -1100), (-1700, -250, -1200), (-1500, -250, -1000), rotation 0 - the round table of the options view (view 11), where the 6 Wobbly Cones stand.
- Control type 6, every frame: steering and throttle are full (±127). While the timer runs, nothing changes; when it runs out, each one is chosen at random (`rand() * 3.05e-5 > 0.5`: +127, else -127) if it is still 0, else it changes its sign; the timer is set to 3 s. The biggest collision impulse of the body this frame (velocity change, units/s) of 400 or more flips both at once and restarts the 3 s. So they drive in circles, forwards and backwards, and turn round at every hard bump.
- Own additions (not in the exe): a car that fell off the table or lies on its roof for 3 s goes back to its start.

### Floor logo

`frontendf.bmp` (page 5 of the frontend) holds the wooden RE-VOLT logo in the texels (0, 128) - (128, 256). It is used by the counter (view 14 looks down on it) and the round table of the options view. The converter replaces it with the project logo carved into the same wood (plain wood rows of the page, a height map lit from the top left).

### Track screen (type 44)

- `bigscreen.m`; the picture is a quad over the corners (-299, -726.4, -75.2), (299, -726.4, -75.2), (299, -291.6, 5), (-299, -291.6, 5) of the screen, uv 0..1. The picture is `gfx/<level>.bmp`, loaded into page 31 / 32 by turns; a change of track zooms / darkens the old one out first. Mirrored / reversed flip the uvs.
- Own: the converter adds 4 frames of tv static (own noise) as hidden quads; the app shows them for 0.35 s on a change of track (with the `tvstatic` sound of the game).
- The 5 Small Screen objects (type 53) are not decoded.

## Laser (game behaviour)

`.fob` type 33 (muse1: 10, muse2: 4), flags width, random, object. No model - drawn in code.

- The beam starts at the object and runs along its look axis. At load a line of 10000 units is tested against the level; the beam ends at the hit. The test is one sided (front faces only): the emitters sit 20-70 units behind the wall surface, two sided the beam would end in that wall. Beams: muse1 7.8-19.3 m, muse2 6.6-7.0 m.
- Flag `object` set: every frame the beam is tested against the cars and objects too and ends at the first one in it. Something in the beam: sparks at the hit point (thrown back along the beam), and the 3D sound of the laser switches from `muse/laserhum` (looped, at the middle of the beam, 22050 Hz) to `muse/alarm2` for 5 s after the last block.
- Drawn as a row of screen space quads, one per 100 units of length, additive with color `0x888888` and the red band of `fxpage1` (u 0.879-0.934, v 0.129-0.184: dark red edges, a light pink core). The width in pixels is `width + random × ((ms + phase) mod 100) / 100` (a 100 ms sawtooth, the phase random per laser) - so it flickers. A glow sprite sits at the end.
- Only while the laser was drawn in the last frame (in view).
- The remake: the converter measures the length one sided like the game (`revolt_laser_length`, m); a beam glb (crossed blades with that band, additive look, tinted red) scaled per frame to its length and to a width of (width + random × sawtooth) × 1.2 cm, a ray per frame against the dynamic bodies for `object`; no sparks yet.

## Piano (game behaviour)

The piano keyboard of toylite. The `.tri` trigger type 0 (one box over the keyboard) only switches the handler on - the keyboard itself is hard coded in the exe (units):

- Front left corner x 1040, z -6525, 2285 wide (along +x), 250 deep (along +z), the keys at y -342.5. 32 white keys of 71.40625 units.
- Black keys: from 112.5 units deep on, a quarter key either side of a key boundary, at the boundaries 1 2 3 5 6 8 9 10 12 13 15 16 17 19 20 22 23 24 26 27 29 30 31.
- Every wheel of a car in the trigger that touches the floor is tested: the key under it lights up for a timer of 16 that runs down by 72 per second (0.22 s); if the timer was 0 it plays the note. So a wheel standing on a key plays it once.
- The note: semitone `n` = the black keys at or left of the key + the key (a black key counts itself once) - `piano` (sound id 0x2B of the toy block) at `12000 + 800 n` Hz, volume 127, at the wheel. Linear in Hz - not a musical scale (the sample has 22050 Hz: speed 0.54 to 2.5).
- The lights: a quad per lit key on the keys (texture page 5 with the shapes of the keys), alpha `12 × int(timer)`, white keys in the colors `ff0c07 ff4700 ffe300 09c31f 0282ff 1000b9 a000e1` by key mod 7, black keys red / green / blue by their index mod 3.
- The trigger of the first frame in the box clears all timers.
- The remake: the converter writes the keyboard and the trigger boxes (m, glTF) as `revolt_piano` and a glb with one light per key (plain quads, the back of a white key notched for its black neighbours).

## Star (game behaviour)

`.fob` type 50, flag type (0 global, 1 practice / stunt):

- Out: it spins about Y at 1/3 turn per second, with an orange light (`0x7F5F00`, radius 512).
- Collected by a car whose box overlaps the star box (±30 units), in race modes only by a car without a weapon and without a running weapon spin. It rises (half the car velocity in x / z, 64 units/s up), grows `1 + 2t` and fades for 1 s, then it is gone - it does not come back. The sound `pickup` plays at the star.
- In a race the car gets weapon 10 (Global Pulse) at once - no spin, 1 shot. The other modes count the stars (practice, stunt arena) instead.
- Game modes in the star handler: 2, 3, 4, 7 races (weapon), 6 battle tag (the star is the tagged thing), 8 practice (sets a 2 s timer - probably a message - and a found flag per level, may unlock a car - not decoded), 9 stunt arena (counts the stars).

## Driving modes (game behaviour)

Select Race -> Championship, Single Race, Clockwork Carnage, Time Trial and Practice go to a menu "Mode" with Simulation, Arcade, Console, Junior RC (strings of `strings/English.txt`); the stunt arena goes straight to the name. The choice (0-3) is copied into the race settings at the start. What reads it (partly decoded):

- Arcade and up: car against car collisions take another path (not decoded).
- Console and up: in the collision response of a car, a contact whose normal is steeper than 0.3 (|n.y| > 0.3, ground hits) gives no angular impulse - bumps and landings do not spin or flip the car.
- Junior RC: at the car setup one car value is halved, another one × 0.6, a value of every wheel × 1.25 (which ones not decoded - speed / steering / grip likely).
- No steering help in any mode.

The remake (own assists, for the player cars): Simulation without the wall slide, Arcade as before (default), Console damps roll and pitch of the upright car (4/s), Junior RC more (8/s) and steers back onto the track near its edges (0.4 s look ahead, the outer 20 % of the width, aim at the middle 6 m ahead), turns a car driving the wrong way around and puts the car back onto the racing line ahead when it is lost: upside down 1.5 s, the wrong way 3 s (after the 1 s of the HUD), more than 3 m off the track 2 s, stuck 3 s (throttle or brake given but no more than 1.5 m away from where it was - jiggling in a corner counts, standing without input for more than 1 s does not). Backing up turns the steering of the help around.

## Practice (game behaviour)

Game mode 8: laps are counted, but the finish check after the last lap is skipped - the run never ends. The HUD leaves out the times box, the wrong way X, the direction signs and the arrow. The practice star (type 1) of the track is out to be found. The remake: alone, no pickups, only speed and fps in the HUD, "Star found!" for 2 s; the unlock is not done.

## Not covered yet

 (car hulls), `Parameters.txt` (car setup), `.cam`, `.lit`, `.vis`, `.fld`, `.por`, `.rpl`.

## Car env map

Decoded from the exe (2026-10-13).
- Only the body model of a car is env mapped (not wheels, springs, axles, aerial), every polygon except the ones with NOENV (1024). Texture: `gfx/envroll.bmp`.
- Drawn as a second pass on top: additive (src ONE, dest ONE), no z write. Vertex color of the pass = `EnvRGB` of Parameters.txt (an option turns it grey).
- UVs per vertex: `u = 0.5 + 0.5 · (n · E)_x`, `v = 0.5 + 0.5 · (n · E)_y`, n = model space vertex normal (row vector), `E = B · Ry(90°) · R`. B = body rotation (rows right / up / look), R = an env matrix per car. No camera in it - a standing car has a fixed reflection.
- R "rolls" with the car: every frame `R = Rot(a, c) · R`, with the position change d of the body (re-volt units): `a = 2π · d.x / 16384`, `c = 2π · d.z / 16384`, `Rot = [[cos c, -sin c, 0], [sin c cos a, cos c cos a, -sin a], [sin a sin c, sin a cos c, cos a]]`.

## Mirror mode (game behaviour)

A level flag (mirrored) next to reversed - there are no files of its own, the game mirrors the loaded level left / right (x of game space): positions and axes of everything, the polygons turned around, even the stereo panning of the sounds is swapped. Reversed and mirrored combine (reversed mirrored). Mirrored tracks use the forward challenge time of time trial.

The remake: the converter writes `<level>[_reversed]_mirrored.scene` with its own asset folder `levels/<level>_mirrored` - world, collision (.ncp), instances and their models, objects and their models / hulls, lights, mirror planes, zones, position nodes, AI nodes (left and right edge swapped, racing line 1 - x), track signs (the left arrows 0-3 become the right ones 4-7 and back), start position (STARTROT negated), sky box, the piano (keys run to -x) and the animated objects (the motion tree mirrored as a whole: translations x negated, rotations (x, -y, -z, w)). Texture animation frames are turned with the polygons that use them.

## Championship (game behaviour)

Select Race -> Championship -> Select Mode -> name -> car -> Select Cup (views 6-9) -> Summary (view 17) -> the races.

| Cup | Races (level, laps, M = mirrored, R = reversed) | Opponents by rating (rookie, amateur, advanced, semi-pro, pro) |
|---|---|---|
| Bronze | Toys in the Hood 1 3, SuperMarket 2 4, Museum 2 3, Botanical Garden 5 | 7, 0, 0, 0, 0 |
| Silver | Toy World 1 6, Toys in the Hood 1 M 4, Ghost Town 1 4, Toy World 2 4 | 0, 4, 3, 0, 0 |
| Gold | Toys in the Hood 2 5, Toy World 1 M 8, Toytanic 1 5, Museum 1 5 | 0, 0, 4, 3, 0 |
| Platinum | Supermarket 1 6, Ghost Town 2 6, Toy World 1 R M 10, Museum 1 M 6, Toytanic 2 6 | 0, 0, 1, 3, 3 |

- The opponents are picked once per cup: per rating that many random cars of that rating (the same candidates as a single race, no car twice; the player's own car is not left out). 8 cars per race.
- Points per place: 10, 6, 4, 3, 2, 1, 0, 0. After a race the table shows them as `+n` (red) and counts them into the total one point every 0.2 s (menuLeftRight at 22050 Hz each); a key puts all in at once. The table is sorted by the total, the same total by the place in the last race.
- Qualified = 3rd or better in the race ("CONGRATULATIONS! YOU QUALIFIED!", after the last race "CONGRATULATIONS! CHAMPIONSHIP FINISHED!"). Worse: "YOU FAILED TO QUALIFY!", "You have n Tries Left.  Try Again?" (Yes / No). 3 tries per cup; a failed race counts nothing; with no try left: "Game Over!  Better Luck Next Time!". Give Up Try (in-race menu) counts as a failed race.
- The cup is won at the top of the table after the last race: the level flag 0x20 of its tracks is set, that opens the next cup (Bronze is always open).
- **Podium** (frontend, views 18-23): 3 s the place in yellow (12 × 24, "%s %s!" = "YOU CAME 1ST!" at y 198, only for 1st-3rd); a first win of a cup adds at y 226 / 254 "SILVER CUP" / "GOLD CUP" / "SPECIAL CUP" (the string of the game for the 4th cup) + "IS UNLOCKED", Platinum: "Well done, you have mastered" / "all Re-Volt Championships!" (the "TRY TO COME HIGHER ..." and "You'll have to do better ..." lines need a flag that is never 0 there - not shown). Then the cars of the table are dropped from 10 m onto the podium places (game units (-1628, -1000, 1763) 1st, (-1801, -1000, 1588) 2nd, (-1448, -1000, 1950) 3rd, facing the cameras): on the podium 3rd, 2nd, 1st, 5 s each (camera: the first quarter stays, the middle half the close up view 22 / 21 / 20, the last quarter the overview 19), with the winner fireworks start (model `firework.m` from x -1884..-1372, y -700..-600, z 1507..2019, one every 0.1-2.6 s); below 3rd all three at once (7 s), then the player's car beside the podium (-1515, -1000, 1665) with a putty bomb (`ball.m`, view 23, 5 s). 3 s (5.5 s below 3rd) later the screen fades and the main menu comes - after the Platinum win the credits.
- The remake: own spark star for the bursts (no particles yet), the bomb goes off after 8 s (puttbang, the car is thrown up).
- Summary of a cup: the cup name in orange, "Place 3rd or better in each / race to progress!", then "Finish top of the table to / access the next challenge!" (Platinum: "Finish top of the table and / be crowned "Re-Volt Master"!").
- The table (640 x 480, chars 8 x 16): x0 = (400 - 56 × race index) / 2; header at y 176 in cyan: "Car" at x0, the race numbers at x0 + 56 i + 152, "Pnts" centered at x0 + 56 race + 212; a row per car from y 208, 16 apart: the name (15 chars) white, the place per race ("1st", green) at x0 + 56 i + 144, the total (2 digits, yellow) and the `+n` (red) behind it. The messages centered at y 100 and 116.

## Best Trial Times (game behaviour)

Main menu -> Best Trial Times (view 15): per track and direction (" (M)", " (R)" behind the name) the 10 best time trial laps, one row each: `MM:SS:mmm NAME CAR` (`%02d:%02d:%03d %16.16s %16.16s`), an empty one `---------` (stored as 30:00:000). The game keeps them in `times/<Normal|Reverse|Mirror|ReverseMirror>/<track name>.Times`: 40 bytes header + 10 × 40 bytes (u32 ms, char name[16], char car[16], 4 bytes).

## Progress Table (game behaviour)

Main menu -> Progress Table (view 10). A flag word per race track (13, the level order of the game):

| Bit | Column | Set when |
|---|---|---|
| 0x10 | Won Race | a race on the track was won |
| 0x01 | Norm | time trial: a lap below the challenge time, forward |
| 0x02 | Rev | the same reversed (its own challenge time) |
| 0x04 | Mirr | the same mirrored (the forward challenge time) |
| 0x08 | Stars | the practice star was found |
| 0x20 | (cup) | the cup of the track was won |

- Layout (x0 60, y0 112): box 520 × 272 and a box 520 × 24 at y0 + 292 ("Stunt Arena - Stars Collected: n of 20"). Header "Tracks", "Won" / "Race", "Norm", "Rev", "Mirr", "Stars" (centered at x0 + 180, 244, 324, 404, 484). Rows 12 apart: a cup row (Bronze / Silver / Gold / Platinum, white, with the cup won mark) before its tracks, the track name in grey (0x808080), the marks (12 × 12) at x0 + 172, 240, 320, 400, 476: a tick (font.bmp 244, 244, 11 × 11) or a cross (231, 244).
- Challenge lap times (s, forward / reversed): Toys in the Hood 1 54 / 54, SuperMarket 2 24 / 24, Museum 2 45 / 45, Botanical Garden 29 / 29, Toy World 1 27 / 34, Ghost Town 1 30 / 30, Toy World 2 37 / 37, Toys in the Hood 2 45 / 48, Toytanic 1 55 / 55, Museum 1 61 / 59, Supermarket 1 39 / 39, Ghost Town 2 38 / 38, Toytanic 2 52 / 52.
- A cup whose tracks all have one flag unlocks something (reversed / mirrored / reverse mirrored tracks, new cars, Clockwork Carnage - the messages are in the strings); not decoded in detail. The remake opens all tracks and directions.
