// the .fob object types: index in the editor object table of revolt.exe (see formats/README.md)
// the models are matched by name - the game picks them in code

pub struct ObjectType
{
    pub id: i32,
    pub name: &'static str,
    // labels of flags[0..4] in the editor, "" = unused
    pub flags: [&'static str; 4],
}

const fn t(id: i32, name: &'static str, flags: [&'static str; 4]) -> ObjectType
{
    ObjectType { id, name, flags }
}

pub const OBJECT_TYPES: &[ObjectType] = &[
    t(0, "Spinning Barrel", ["speed", "", "", ""]),
    t(1, "Beachball", ["", "", "", ""]),
    t(2, "Planet", ["name", "orbit", "orbit speed", "spin speed"]),
    t(3, "Plane", ["speed", "radius", "bank", ""]),
    t(4, "Copter", ["x range", "y range", "z range", "y offset"]),
    t(5, "Dragon", ["", "", "", ""]),
    t(6, "Water", ["", "", "", ""]),
    t(7, "Trolley", ["", "", "", ""]),
    t(8, "Boat", ["type", "", "", ""]),
    t(9, "Speedup", ["width", "low speed", "high speed", "time"]),
    t(10, "Radar", ["", "", "", ""]),
    t(11, "Balloon", ["", "", "", ""]),
    t(12, "Horse", ["", "", "", ""]),
    t(13, "Train", ["", "", "", ""]),
    t(14, "Strobe", ["type", "sequence num", "sequence count", ""]),
    t(15, "Football", ["", "", "", ""]),
    t(16, "Spark Generator", ["type", "av. speed", "var. speed", "frequency"]),
    t(17, "Space Man", ["", "", "", ""]),
    t(30, "Pickup", ["", "", "", ""]),
    t(32, "Flappage", ["", "", "", ""]),
    t(33, "Laser", ["width", "rand", "object", ""]),
    t(36, "Wobbly Cone", ["", "", "", ""]),
    t(37, "Probe Logo", ["", "", "", ""]),
    t(38, "Clouds", ["", "", "", ""]),
    t(39, "Name Entry", ["", "", "", ""]),
    t(40, "Sprinkler", ["id", "", "", ""]),
    t(41, "Sprinkler Hose", ["id", "", "", ""]),
    t(42, "Object Thrower", ["id", "object", "speed", "reuse"]),
    t(43, "BasketBall", ["", "", "", ""]),
    t(44, "TrackScreen", ["", "", "", ""]),
    t(45, "Clock", ["", "", "", ""]),
    t(46, "CarBox", ["car id", "", "", ""]),
    t(47, "Stream", ["model", "", "", ""]),
    t(48, "Cup", ["type", "", "", ""]),
    t(49, "3D Sound", ["name", "range", "random play", ""]),
    t(50, "Star", ["type", "", "", ""]),
    t(52, "Tumbleweed", ["", "", "", ""]),
    t(53, "Small Screen", ["", "", "", ""]),
    t(54, "Lantern", ["", "", "", ""]),
    t(55, "Skybox", ["level", "", "", ""]),
    t(56, "Slider", ["id", "", "", ""]),
    t(57, "Bottle", ["stop", "", "", ""]),
    t(58, "Bucket", ["", "", "", ""]),
    t(59, "Cone", ["", "", "", ""]),
    t(60, "Can", ["", "", "", ""]),
    t(61, "Lilo", ["", "", "", ""]),
    t(63, "Rain", ["", "", "", ""]),
    t(64, "Lightning", ["", "", "", ""]),
    t(65, "Ship light", ["", "", "", ""]),
    t(66, "Packet", ["", "", "", ""]),
    t(67, "ABC Block", ["", "", "", ""]),
    t(68, "Water Box", ["x range", "y range", "z range", ""]),
    t(69, "Water Ripples", ["model", "", "", ""]),
    t(70, "Gari flag", ["garyness", "", "", ""]),
    t(71, "Dolphin", ["", "", "", ""]),
    t(72, "Garden Fog", ["", "", "", ""]),
    t(73, "Fog Box", ["x range", "y range", "z range", ""]),
];

pub fn object_type(id: i32) -> Option<&'static ObjectType>
{
    OBJECT_TYPES.iter().find(|t| t.id == id)
}

const PLANETS: [&str; 11] = ["mercury", "venus", "earth", "mars", "jupiter", "saturn", "uranus", "neptune", "pluto", "moon", "rings"];

// the main model of an object, relative to the game folder - None for objects without one (sounds, fog, sparks, ...)
// only the body: rotors, wheels, sprinkler heads and so on are moving parts the game adds in code
// the exe loads some object models with a page offset added to their textured polygons: beachball + speedup 27 (gfx/fxpage1), dragon 31 (toy2/dragon.bmp)
pub fn page_offset(kind: i32) -> i16
{
    match kind
    {
        1 | 9 => 27,
        5 => 31,
        _ => 0,
    }
}

// the moving parts the exe draws besides the model (formats/README.md, Object motion) - dragon3 is the morph target of dragon2
pub fn parts(kind: i32, level: &str) -> Vec<String>
{
    let models = |names: &[&str]| names.iter().map(|name| format!("models/{name}.m")).collect();
    match kind
    {
        3 => models(&["plane2"]),
        4 => models(&["copter2", "copter3"]),
        5 => models(&["dragon2", "dragon3"]),
        13 => models(&["train2", "train3"]),
        // the name wheel turns in front of the name stand
        39 => vec!["levels/frontend/namewheel.m".to_string()],
        40 => models(&[if level.to_lowercase().starts_with("garden") { "spheadgarden" } else { "sphead" }]),
        _ => vec![],
    }
}

pub fn model(kind: i32, flags: &[i32; 4], level: &str) -> Option<String>
{
    let level = level.to_lowercase();
    let models = |name: &str| Some(format!("models/{name}.m"));
    let index = |i: i32| usize::try_from(i).ok();

    match kind
    {
        0 => models("barrel"),
        1 => models("beachball"),
        // name 11 is most likely the sun - no model for it
        2 => index(flags[0]).and_then(|i| PLANETS.get(i)).and_then(|name| models(name)),
        3 => models("plane"),
        4 => models("copter"),
        5 => models("dragon1"),
        6 => models("water"),
        7 => Some("cars/trolley/trollbod.m".to_string()),
        8 => models(&format!("boat{}", flags[0] + 1)),
        9 => models("speedup"),
        10 => models("radar"),
        11 => models("balloon"),
        12 => models("horse"),
        13 => models("train"),
        14 => models(&format!("light{}", flags[0] + 1)),
        15 => models("football"),
        17 => models("spaceman"),
        30 => models("pickup"),
        32 => models("flap"),
        36 => models("cone"),
        37 => models("probe"),
        39 => Some("levels/frontend/namestand.m".to_string()),
        40 => models(if level.starts_with("garden") { "spbasegarden" } else { "spbase" }),
        41 => models("sphose"),
        43 => models("basketball"),
        44 => Some("levels/frontend/bigscreen.m".to_string()),
        45 => Some("levels/frontend/clockbody.m".to_string()),
        46 => Some("levels/frontend/carbox.m".to_string()),
        // model 1 (ship levels) is unknown
        47 if flags[0] == 0 => models("stream"),
        48 => Some(format!("levels/frontend/cup0{}.m", flags[0] + 1)),
        50 => models("star"),
        52 => models("tumble_weed"),
        53 => Some("levels/frontend/smallscreen.m".to_string()),
        54 => models("lantern"),
        56 => models("slider"),
        57 => models("bottle"),
        58 => models("bucket"),
        59 => models(if level == "nhood2" { "n2trafficcone" } else { "trafficcone" }),
        65 => models("shiplight"),
        66 => models("packet"),
        67 => models("abcblock"),
        // model 2..5 -> gardenwater1..4 (sizes fit, not checked in the game), 1 most likely the toytanic pool
        69 => match flags[0]
        {
            1 => models("poolwater"),
            2..=5 => models(&format!("gardenwater{}", flags[0] - 1)),
            _ => None,
        },
        _ => None,
    }
}
