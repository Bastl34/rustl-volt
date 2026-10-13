// the game's mirror mode: the level mirrored left / right (x of re-volt space) right after loading, the rest of the conversion is unchanged
// rotations stay rotations: rows right, up, look become -S·right, S·up, S·look for the mirrored model (S = x negated)
// polygons keep corner 0 and run the other way round (front faces stay front faces)

use serde_json::Value;

use super::instances::LevelInstances;
use super::objects::LevelObjects;
use super::Log;
use crate::revolt::fan::AiNodes;
use crate::revolt::fob::Object;
use crate::revolt::hul::Hull;
use crate::revolt::lit::Light;
use crate::revolt::mesh::{Mesh, Polygon, FACE_TEXANIM};
use crate::revolt::ncp::{Ncp, Plane};
use crate::revolt::pan::TrackPath;
use crate::revolt::rim::MirrorPlane;
use crate::revolt::taz::Zone;
use crate::revolt::tri::{Trigger, TRIGGER_TRACK_DIR};
use crate::revolt::world::{TexFrame, World};

pub fn point(p: [f32; 3]) -> [f32; 3]
{
    [-p[0], p[1], p[2]]
}

// rows right, up, look
pub fn axes(axes: [[f32; 3]; 3]) -> [[f32; 3]; 3]
{
    let [right, up, look] = axes;
    [[right[0], -right[1], -right[2]], point(up), point(look)]
}

fn plane(plane: &mut Plane)
{
    plane.normal = point(plane.normal);
}

fn bounds(min: &mut [f32; 3], max: &mut [f32; 3])
{
    (min[0], max[0]) = (-max[0], -min[0]);
}

pub fn mesh(mesh: &mut Mesh)
{
    for vertex in &mut mesh.vertices
    {
        vertex.position = point(vertex.position);
        vertex.normal = point(vertex.normal);
    }
    for polygon in &mut mesh.polygons
    {
        let n = polygon.corners();
        polygon.indices[1..n].reverse();
        polygon.colors[1..n].reverse();
        polygon.uvs[1..n].reverse();
    }
}

pub fn world(world: &mut World)
{
    for world_mesh in &mut world.meshes
    {
        world_mesh.center = point(world_mesh.center);
        mesh(&mut world_mesh.mesh);
    }
    for cube in &mut world.big_cubes
    {
        cube.center = point(cube.center);
    }
}

// the frames of a texture animation hold the uvs per corner - turned like the polygons that use them (after those were mirrored)
pub fn tex_anims<'a>(anims: &mut [Vec<TexFrame>], polygons: impl Iterator<Item = &'a Polygon>, log: &Log)
{
    let mut corners: Vec<Option<usize>> = vec![None; anims.len()];
    let mut mixed = 0;
    for polygon in polygons.filter(|p| p.has(FACE_TEXANIM))
    {
        let Some(slot) = usize::try_from(polygon.texture).ok().and_then(|i| corners.get_mut(i)) else { continue; };
        match *slot
        {
            Some(n) if n != polygon.corners() => mixed += 1,
            _ => *slot = Some(polygon.corners()),
        }
    }
    for (frames, n) in anims.iter_mut().zip(corners)
    {
        let n = n.unwrap_or(4);
        frames.iter_mut().for_each(|frame| frame.uvs[1..n].reverse());
    }
    if mixed > 0 { log.warn(format!("  mirrored: {mixed} texture animated polygons share an animation with polygons of another corner count - their first frame is off")); }
}

pub fn ncp(ncp: &mut Ncp)
{
    for polygon in &mut ncp.polygons
    {
        plane(&mut polygon.plane);
        polygon.edges.iter_mut().for_each(plane);
        bounds(&mut polygon.min, &mut polygon.max);
    }
    if let Some(grid) = ncp.grid.as_mut()
    {
        grid.x0 = -(grid.x0 + grid.x_count as f32 * grid.raster);
    }
}

pub fn hull(hull: &mut Hull)
{
    for convex in &mut hull.hulls
    {
        convex.vertices.iter_mut().for_each(|v| *v = point(*v));
        convex.faces.iter_mut().for_each(plane);
        convex.offset = point(convex.offset);
    }
    for sphere in &mut hull.spheres
    {
        sphere.center = point(sphere.center);
    }
}

pub fn instances(level: &mut LevelInstances)
{
    for instance in &mut level.instances
    {
        instance.position = point(instance.position);
        instance.axes = axes(instance.axes);
    }
    for model in &mut level.models
    {
        mesh(&mut model.mesh);
        if let Some(model_ncp) = model.ncp.as_mut() { ncp(model_ncp); }
    }
}

pub fn object(object: &mut Object)
{
    object.position = point(object.position);
    object.up = point(object.up);
    object.look = point(object.look);
}

pub fn objects(level: &mut LevelObjects)
{
    level.objects.iter_mut().for_each(object);
    for model in &mut level.models
    {
        mesh(&mut model.mesh);
    }
}

pub fn lights(lights: &mut [Light])
{
    for light in lights
    {
        light.position = point(light.position);
        light.axes = axes(light.axes);
    }
}

pub fn mirror_planes(planes: &mut [MirrorPlane])
{
    for mirror_plane in planes
    {
        mirror_plane.normal = point(mirror_plane.normal);
        mirror_plane.corners = mirror_plane.corners.map(point);
    }
}

pub fn zones(zones: &mut [Zone])
{
    for zone in zones
    {
        zone.center = point(zone.center);
        zone.axes = axes(zone.axes);
    }
}

pub fn triggers(triggers: &mut [Trigger])
{
    for trigger in triggers
    {
        trigger.center = point(trigger.center);
        trigger.axes = axes(trigger.axes);
        // the arrows: 0-3 the left ones, 4-7 the same to the right
        if trigger.kind == TRIGGER_TRACK_DIR && (0..8).contains(&(trigger.flag & 0xffff)) { trigger.flag ^= 4; }
    }
}

pub fn track_path(track: &mut TrackPath)
{
    for node in &mut track.nodes
    {
        node.position = point(node.position);
    }
}

// left and right edge change places, the racing line (0 left .. 1 right) with them
pub fn ai_nodes(ai: &mut AiNodes)
{
    for node in &mut ai.nodes
    {
        (node.left, node.right) = (point(node.right), point(node.left));
        (node.left_speed, node.right_speed) = (node.right_speed, node.left_speed);
        node.racing_line = 1.0 - node.racing_line;
        node.line2 = 1.0 - node.line2;
    }
}

// the .inf values on the level root (glTF space): start positions x and the turns around the up axis
pub fn level_extras(extras: &mut Value)
{
    let Some(map) = extras.as_object_mut() else { return; };
    for key in ["start_pos_x", "start_pos_rev_x", "start_rot_y", "start_rot_rev_y"]
    {
        if let Some(v) = map.get(key).and_then(|v| v.as_f64()) { map.insert(key.into(), serde_json::json!(-v)); }
    }
}
