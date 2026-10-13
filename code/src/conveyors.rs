// the moving surfaces of the .ncp (escalators of muse2, conveyors of the market): the converter writes their velocity onto the collision node (revolt_conveyor_x / y / z, m/s)
// the engine lets the wheels grip relative to them - with the belt faster, against it slower

use rustl::prelude::*;

#[derive(Default)]
pub struct Conveyors
{
    // scene id and node id of every surface given a velocity
    surfaces: Vec<(u32, u32)>,
}

impl Conveyors
{
    pub fn add_scene(&mut self, scene: &mut Scene)
    {
        let mut found = vec![];
        crate::for_each_node(&scene.nodes, &mut |node|
        {
            let velocity =
            {
                let read = node.read().unwrap();
                let axis = |key: &str| read.extras.get::<f64>(key).map(|v| *v as f32);
                match (axis("revolt_conveyor_x"), axis("revolt_conveyor_y"), axis("revolt_conveyor_z"))
                {
                    (Some(x), Some(y), Some(z)) => Some(Vector3::new(x, y, z)),
                    _ => None,
                }
            };
            // the collider may sit on a child (one node per primitive)
            if let Some(velocity) = velocity
            {
                crate::for_each_node(&vec![node.clone()], &mut |part| found.push((part.read().unwrap().id, velocity)));
            }
        });

        for (id, velocity) in &found
        {
            if self.surfaces.contains(&(scene.id, *id)) { continue; }
            scene.physics.set_surface_velocity(*id, Some(*velocity));
            self.surfaces.push((scene.id, *id));
        }
        if !found.is_empty()
        {
            console_log!("scene '{}': {} moving surfaces (conveyors / escalators)", scene.name, found.len());
        }
    }

    // the scene was unloaded - its physics went with it
    pub fn remove_scene(&mut self, scene_id: u32)
    {
        self.surfaces.retain(|(id, _)| *id != scene_id);
    }

    pub fn stop(&mut self, state: &mut State)
    {
        for (scene_id, id) in self.surfaces.drain(..)
        {
            if let Some(scene) = state.find_scene_by_id_mut(scene_id)
            {
                scene.physics.set_surface_velocity(id, None);
            }
        }
    }
}
