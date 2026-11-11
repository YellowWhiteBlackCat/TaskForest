import bevy_ui_render::ui_vertex_output::UiVertexOutput;

struct OpticalGlassUniform {
    optics: vec4<f32>,
    finish: vec4<f32>,
    tint: vec4<f32>,
    grain: vec4<f32>,
    element: vec4<f32>,
    viewport: vec4<f32>,
    refraction: vec2<f32>,
}

@group(1) @binding(0)
var<uniform> glass: OpticalGlassUniform;

@group(1) @binding(1)
var backdrop: texture_2d<f32>;

@group(1) @binding(2)
var backdrop_sampler: sampler;

@group(1) @binding(3)
var grain_texture: texture_2d<f32>;

@group(1) @binding(4)
var grain_sampler: sampler;

const GRAIN_TILE_PX: f32 = 64.0;

fn rounded_box_sdf(point: vec2<f32>, half_size: vec2<f32>, radius: f32) -> f32 {
    let q = abs(point) - half_size + vec2<f32>(radius);
    return length(max(q, vec2<f32>(0.0)))
        + min(max(q.x, q.y), 0.0)
        - radius;
}

fn blurred_backdrop(uv: vec2<f32>, radius_px: f32) -> vec4<f32> {
    let step_px = max(radius_px, 1.0) * 0.5 / glass.viewport.xy;
    var sum = vec4<f32>(0.0);
    var total = 0.0;
    for (var y = -2; y <= 2; y = y + 1) {
        for (var x = -2; x <= 2; x = x + 1) {
            let offset = vec2<f32>(f32(x), f32(y)) * step_px;
            let weight = exp(-0.5 * f32(x * x + y * y) / 4.0);
            sum = sum + textureSampleLevel(
                backdrop,
                backdrop_sampler,
                uv + offset,
                0.0,
            ) * weight;
            total = total + weight;
        }
    }
    return sum / total;
}

// One Snell step: the incident view ray bends through the rounded-SDF
// surface normal, scaled by the glass IOR the uniform states. The lateral
// component is negated so samples pull toward the edge — the
// magnifying-glass reading the proven pipeline established.
fn refracted_offset(normal2: vec2<f32>, edge_band: f32, strength_px: f32) -> vec2<f32> {
    let incident = vec3<f32>(0.0, 0.0, -1.0);
    let surface = normalize(vec3<f32>(normal2 * edge_band, 1.0));
    let ratio = 1.0 / max(glass.refraction.x, 1.0);
    let bent = refract(incident, surface, ratio);
    return -bent.xy * strength_px;
}

fn fresnel_rim(edge_band: f32) -> f32 {
    let cos_theta = 1.0 - edge_band;
    let grazing = pow(1.0 - clamp(cos_theta, 0.0, 1.0), 5.0);
    return glass.refraction.y + (1.0 - glass.refraction.y) * grazing;
}

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    let half_size = in.size * 0.5;
    let point = (in.uv - vec2<f32>(0.5)) * in.size;
    let distance = rounded_box_sdf(point, half_size, glass.viewport.z);

    let epsilon = 1.0;
    let dx = rounded_box_sdf(
        point + vec2<f32>(epsilon, 0.0),
        half_size,
        glass.viewport.z,
    ) - rounded_box_sdf(
        point - vec2<f32>(epsilon, 0.0),
        half_size,
        glass.viewport.z,
    );
    let dy = rounded_box_sdf(
        point + vec2<f32>(0.0, epsilon),
        half_size,
        glass.viewport.z,
    ) - rounded_box_sdf(
        point - vec2<f32>(0.0, epsilon),
        half_size,
        glass.viewport.z,
    );
    let gradient = vec2<f32>(dx, dy);
    let normal = gradient / max(length(gradient), 0.0001);

    let inside = -distance;
    let refraction_edge = 1.0 - smoothstep(
        0.0,
        max(glass.optics.z, 1.0),
        inside,
    );
    let rim_width = max(glass.finish.x * 2.0, 2.5);
    let specular_rim = 1.0 - smoothstep(
        0.0,
        rim_width,
        inside,
    );
    let fresnel = fresnel_rim(refraction_edge) * specular_rim;
    let mask = 1.0 - smoothstep(-1.0, 0.0, distance);
    let backdrop_uv = (glass.element.xy + in.uv * glass.element.zw)
        / glass.viewport.xy;
    let shift = refracted_offset(normal, refraction_edge, glass.optics.y)
        / glass.viewport.xy;
    let dispersion = glass.optics.w;

    let red = blurred_backdrop(
        backdrop_uv + shift * (1.0 + dispersion),
        glass.optics.x,
    ).r;
    let green = blurred_backdrop(
        backdrop_uv + shift,
        glass.optics.x,
    ).g;
    let blue = blurred_backdrop(
        backdrop_uv + shift * (1.0 - dispersion),
        glass.optics.x,
    ).b;
    let refracted = vec3<f32>(red, green, blue);

    let key_light = normalize(vec2<f32>(-0.55, -0.84));
    let fill_light = normalize(vec2<f32>(0.55, 0.84));
    let key_dot = max(dot(normal, key_light), 0.0);
    let fill_dot = max(dot(normal, fill_light), 0.0);
    let key_specular = pow(key_dot, 12.0) * fresnel * glass.finish.y
        + key_dot * fresnel * (glass.finish.y * 0.35);
    let fill_specular = pow(fill_dot, 8.0) * fresnel * (glass.finish.y * 0.25)
        + fill_dot * fresnel * (glass.finish.y * 0.12);
    let specular = key_specular + fill_specular;

    let inner_rim = 1.0 - smoothstep(0.5, max(glass.optics.z * 0.35, 4.0), inside);
    let inner_glow = inner_rim * 0.08 * glass.finish.y
        * (1.0 + glass.refraction.y);
    let dispersion_fringe = vec3<f32>(
        key_dot * 0.06,
        (key_dot + fill_dot) * 0.02,
        fill_dot * 0.08,
    ) * specular_rim * glass.optics.w;
    let sheen_diag = in.uv.x * 0.45 + in.uv.y * 0.55;
    let sheen = pow(max(0.0, 1.0 - sheen_diag), 2.5) * 0.06 * glass.finish.y;
    var color = mix(refracted, glass.tint.rgb, glass.tint.a)
        + vec3<f32>(specular + inner_glow + sheen)
        + dispersion_fringe;

    let grain_uv = in.uv * glass.element.zw / vec2<f32>(GRAIN_TILE_PX);
    let grain_noise = textureSample(grain_texture, grain_sampler, grain_uv).r;
    let grain_amount = clamp(1.0 - grain_noise, 0.0, 1.0)
        * glass.grain.w;
    color = mix(color, glass.grain.rgb, grain_amount);

    return vec4<f32>(color, mask * glass.finish.z);
}
