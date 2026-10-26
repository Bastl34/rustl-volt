// the .fob object types (formats/README.md) - the converter writes the number as revolt_object_type into the extras

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ObjectType
{
    SpinningBarrel = 0,
    Beachball = 1,
    Planet = 2,
    Plane = 3,
    Copter = 4,
    Dragon = 5,
    Water = 6,
    Trolley = 7,
    Boat = 8,
    Speedup = 9,
    Radar = 10,
    Balloon = 11,
    Horse = 12,
    Train = 13,
    Strobe = 14,
    Football = 15,
    SparkGenerator = 16,
    SpaceMan = 17,
    Pickup = 30,
    Flappage = 32,
    Laser = 33,
    WobblyCone = 36,
    ProbeLogo = 37,
    Clouds = 38,
    NameEntry = 39,
    Sprinkler = 40,
    SprinklerHose = 41,
    ObjectThrower = 42,
    BasketBall = 43,
    TrackScreen = 44,
    Clock = 45,
    CarBox = 46,
    Stream = 47,
    Cup = 48,
    Sound3d = 49,
    Star = 50,
    Tumbleweed = 52,
    SmallScreen = 53,
    Lantern = 54,
    Skybox = 55,
    Slider = 56,
    Bottle = 57,
    Bucket = 58,
    Cone = 59,
    Can = 60,
    Lilo = 61,
    Rain = 63,
    Lightning = 64,
    ShipLight = 65,
    Packet = 66,
    AbcBlock = 67,
    WaterBox = 68,
    WaterRipples = 69,
    GariFlag = 70,
    Dolphin = 71,
    GardenFog = 72,
    FogBox = 73,
}

const ALL: [ObjectType; 57] = [
    ObjectType::SpinningBarrel, ObjectType::Beachball, ObjectType::Planet, ObjectType::Plane, ObjectType::Copter, ObjectType::Dragon,
    ObjectType::Water, ObjectType::Trolley, ObjectType::Boat, ObjectType::Speedup, ObjectType::Radar, ObjectType::Balloon,
    ObjectType::Horse, ObjectType::Train, ObjectType::Strobe, ObjectType::Football, ObjectType::SparkGenerator, ObjectType::SpaceMan,
    ObjectType::Pickup, ObjectType::Flappage, ObjectType::Laser, ObjectType::WobblyCone, ObjectType::ProbeLogo, ObjectType::Clouds,
    ObjectType::NameEntry, ObjectType::Sprinkler, ObjectType::SprinklerHose, ObjectType::ObjectThrower, ObjectType::BasketBall,
    ObjectType::TrackScreen, ObjectType::Clock, ObjectType::CarBox, ObjectType::Stream, ObjectType::Cup, ObjectType::Sound3d,
    ObjectType::Star, ObjectType::Tumbleweed, ObjectType::SmallScreen, ObjectType::Lantern, ObjectType::Skybox, ObjectType::Slider,
    ObjectType::Bottle, ObjectType::Bucket, ObjectType::Cone, ObjectType::Can, ObjectType::Lilo, ObjectType::Rain,
    ObjectType::Lightning, ObjectType::ShipLight, ObjectType::Packet, ObjectType::AbcBlock, ObjectType::WaterBox,
    ObjectType::WaterRipples, ObjectType::GariFlag, ObjectType::Dolphin, ObjectType::GardenFog, ObjectType::FogBox,
];

impl ObjectType
{
    pub fn from_id(id: i64) -> Option<ObjectType>
    {
        ALL.iter().copied().find(|t| *t as i64 == id)
    }

    pub fn is_water(&self) -> bool
    {
        matches!(self, ObjectType::Water | ObjectType::Stream | ObjectType::WaterRipples)
    }

    pub fn is_ball(&self) -> bool
    {
        matches!(self, ObjectType::Beachball | ObjectType::Football | ObjectType::BasketBall)
    }
}
