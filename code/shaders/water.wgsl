// @transparent
// the water objects of the levels: rings where drops fall in, a calm shimmer everywhere, the sky mirrored by fresnel
// the zones come from the game every frame (ripples.rs): x, z (world m), radius m, strength 0..1 - the rings start at random points inside

// @param zone0: vec4 = 0,0,0,0 (-100..100)
// @param zone1: vec4 = 0,0,0,0 (-100..100)
// @param zone2: vec4 = 0,0,0,0 (-100..100)
// @param zone3: vec4 = 0,0,0,0 (-100..100)
// @param sky_color: color = 0.45,0.7,0.95,1
// @param horizon_color: color = 0.85,0.92,1,1
// @param ripples: f32 = 1 (0..3)
// @param calm: f32 = 1 (0..3)
// @param reflection: f32 = 0.7 (0..1)

const RIPPLE_ZONES: i32 = 4;
// rings per zone at once at full strength
const RIPPLE_RINGS: i32 = 16;
// s a ring runs, m its front gets out, wave length and width of the wave packet (m), tilt of the surface at full height
const RIPPLE_LIFE: f32 = 2.5;
const RIPPLE_REACH: f32 = 3.6;
const RIPPLE_WAVELENGTH: f32 = 0.6;
const RIPPLE_WIDTH: f32 = 0.7;
const RIPPLE_TILT: f32 = 1.0;

fn ripple_hash(n: u32) -> u32
{
    var x = n * 747796405u + 2891336453u;
    x = ((x >> ((x >> 28u) + 4u)) ^ x) * 277803737u;
    return (x >> 22u) ^ x;
}

fn ripple_random(n: u32) -> f32
{
    return f32(ripple_hash(n)) / 4294967295.0;
}

fn ripple_zone(i: i32) -> vec4<f32>
{
    switch i
    {
        case 0: { return param_zone0(); }
        case 1: { return param_zone1(); }
        case 2: { return param_zone2(); }
        default: { return param_zone3(); }
    }
}

// slope (x, z) of the rings of one zone at p
fn ripple_rings(p: vec2<f32>, zone: vec4<f32>, index: i32) -> vec2<f32>
{
    var slope = vec2<f32>(0.0);
    let k = 2.0 * PI / RIPPLE_WAVELENGTH;
    for (var i = 0; i < RIPPLE_RINGS; i += 1)
    {
        let t = get_time() / RIPPLE_LIFE + f32(i) / f32(RIPPLE_RINGS);
        let age = fract(t);
        let seed = ripple_hash(u32(floor(t)) * 1664525u + u32(i) * 1013904223u + u32(index) * 2246822519u);

        // a weak zone: only some of the rings
        if (ripple_random(seed) > zone.w)
        {
            continue;
        }

        let angle = ripple_random(seed + 1u) * 2.0 * PI;
        let center = zone.xy + vec2<f32>(cos(angle), sin(angle)) * sqrt(ripple_random(seed + 2u)) * zone.z;
        let offset = p - center;
        let distance = length(offset);
        let front = age * RIPPLE_REACH;
        let behind = distance - front;
        if (abs(behind) > RIPPLE_WIDTH * 3.0)
        {
            continue;
        }

        // fades with age and spreads thinner the wider it gets
        let fade = (1.0 - age) * (1.0 - age) / (1.0 + distance * 0.6);
        let envelope = exp(-(behind * behind) / (RIPPLE_WIDTH * RIPPLE_WIDTH)) * fade;
        slope += offset / max(distance, 0.0001) * cos(behind * k) * envelope;
    }
    return slope * RIPPLE_TILT;
}

// a few slow waves over the whole water
fn ripple_calm(p: vec2<f32>) -> vec2<f32>
{
    let t = get_time();
    var slope = vec2<f32>(0.0);
    slope += vec2<f32>(0.8, 0.6) * cos(dot(p, vec2<f32>(0.8, 0.6)) * 9.0 + t * 1.7);
    slope += vec2<f32>(-0.5, 0.87) * cos(dot(p, vec2<f32>(-0.5, 0.87)) * 13.0 - t * 2.1);
    slope += vec2<f32>(0.2, -0.98) * cos(dot(p, vec2<f32>(0.2, -0.98)) * 6.0 + t * 1.2);
    return slope * 0.015;
}

fn ripple_slope(p: vec2<f32>, footprint: f32) -> vec2<f32>
{
    var slope = ripple_calm(p) * param_calm() * (1.0 - smoothstep(0.1, 0.4, footprint));

    // the rings go where a pixel covers more than a wave (no shimmer in the distance)
    let detail = 1.0 - smoothstep(RIPPLE_WAVELENGTH * 0.2, RIPPLE_WAVELENGTH * 0.7, footprint);
    if (detail > 0.0)
    {
        for (var z = 0; z < RIPPLE_ZONES; z += 1)
        {
            let zone = ripple_zone(z);
            if (zone.w <= 0.001 || length(p - zone.xy) > zone.z + RIPPLE_REACH + RIPPLE_WIDTH * 3.0)
            {
                continue;
            }
            slope += ripple_rings(p, zone, z) * param_ripples() * detail;
        }
    }
    return slope;
}

// churned water where the drops come down - also seen from far away where the rings fade
fn ripple_froth(p: vec2<f32>) -> f32
{
    var froth = 0.0;
    for (var z = 0; z < RIPPLE_ZONES; z += 1)
    {
        let zone = ripple_zone(z);
        let d = length(p - zone.xy) / max(zone.z, 0.01);
        froth += zone.w * exp(-d * d);
    }
    return froth;
}

fn custom_surface(in: CustomSurfaceInput, surface: ptr<function, CustomSurface>)
{
    // looking flat over it the mirror covers more
    let facing = clamp(dot(in.normal, in.view_dir), 0.0, 1.0);
    let fresnel = pow(1.0 - facing, 5.0);
    (*surface).base_color.a = mix((*surface).base_color.a, 1.0, fresnel * param_reflection());
}

fn custom_fragment(in: CustomSurfaceInput, color: vec3<f32>) -> vec3<f32>
{
    let p = in.position.xz;
    let footprint = length(fwidth(p));
    let slope = ripple_slope(p, footprint);
    var n = normalize(in.normal - vec3<f32>(slope.x, 0.0, slope.y));
    // tilted away from the camera the mirror would look into the water
    n = normalize(n + in.view_dir * max(0.0, 0.05 - dot(n, in.view_dir)));

    let facing = clamp(dot(n, in.view_dir), 0.0, 1.0);
    let fresnel = 0.02 + 0.98 * pow(1.0 - facing, 5.0);
    let reflected = reflect(-in.view_dir, n);
    let sky = mix(param_horizon_color().rgb, param_sky_color().rgb, sqrt(clamp(reflected.y, 0.0, 1.0)));
    let froth = ripple_froth(p) * 0.2 * param_ripples();
    return mix(color, sky, fresnel * param_reflection()) + param_horizon_color().rgb * froth;
}
