// Liquid paint: faceted toon lighting with a stylised highlight, a Fresnel
// contour and slow internal ripples. Parameters come from `liquid.rs`
// (`LiquidParams`), themselves taken from `style.rs`.
//
// Readability rules (competitive integrity):
// - the contour and the light floor are present at every quality level, so
//   the silhouette reads the same on Low and Ultra and in dark corners;
// - nothing here draws through walls (depth test untouched, opaque);
// - the body never becomes transparent while alive.

import bevy_pbr::render::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::main_pass_post_lighting_processing,
    mesh_view_bindings::{view, lights, globals},
    mesh_view_types::DIRECTIONAL_LIGHT_FLAGS_SHADOWS_ENABLED_BIT,
    mesh_types::MESH_FLAGS_SHADOW_RECEIVER_BIT,
    shadows::fetch_directional_shadow,
};

struct LiquidParams {
    primary: vec4<f32>,
    secondary: vec4<f32>,
    contour: vec4<f32>,
    // contour intensity, highlight, roughness, opacity
    look: vec4<f32>,
    // ripple speed, deformation, light bands, quality (0..3)
    motion: vec4<f32>,
    // melt 0..1, respawn front 0..1 (negative = off), hit flash 0..1, seed
    state: vec4<f32>,
};

@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> liquid: LiquidParams;

const PI: f32 = 3.14159265;

fn hash13(p: vec3<f32>) -> f32 {
    var q = fract(p * 0.1031);
    q += dot(q, q.zyx + 31.32);
    return fract((q.x + q.y) * q.z);
}

// Antialiased quantisation of `x` (0..1) into `n` bands.
fn bands(x: f32, n: f32) -> f32 {
    let s = x * n;
    let f = floor(s);
    let w = max(fwidth(s), 1e-3);
    let step_part = smoothstep(0.5 - w, 0.5 + w, fract(s));
    return clamp((f + step_part) / n, 0.0, 1.0);
}

@fragment
fn fragment(
    mesh: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    let pbr_input = pbr_input_from_standard_material(mesh, is_front);
    let quality = liquid.motion.w;
    let t = globals.time;

    var bindp = vec3<f32>(0.5, 0.5, 0.5);
    var region = 0.0;
    @if(VERTEX_COLORS) {
        bindp = mesh.color.xyz;
        region = mesh.color.w;
    }
    var height = 0.5;
    var facet = 0.5;
    @if(VERTEX_UVS_A) {
        height = mesh.uv.x;
        facet = mesh.uv.y;
    }
    // Body-space position in game units: ripples stick to the body.
    let bp = (bindp - vec3<f32>(0.5, 0.0, 0.5)) * 80.0;

    // Melting (death): eaten from the top with a ragged, dripping front.
    let melt = liquid.state.x;
    var melt_edge = 0.0;
    if melt > 0.0 {
        let n = hash13(floor(bp * 0.6) + liquid.state.w) * 0.09;
        let drip = sin(bp.x * 1.7 + bp.z * 2.3) * 0.03;
        let front = 1.08 - melt * 1.2 + n + drip;
        if height > front {
            discard;
        }
        melt_edge = smoothstep(front - 0.05, front, height);
    }

    // Mesh normal: mostly smooth (rounded body) with a hint of the facet
    // baked in by `mesh.rs` (FACET_SMOOTHING), skinned with the body.
    let facet_n = normalize(cross(dpdx(mesh.world_position.xyz), dpdy(mesh.world_position.xyz)));
    var n = normalize(mesh.world_normal);
    if dot(mesh.world_normal, mesh.world_normal) < 1e-8 {
        n = facet_n;
    }
    let v = normalize(view.world_position.xyz - mesh.world_position.xyz);
    if dot(n, v) < 0.0 {
        n = -n;
    }
    let ndv = clamp(dot(n, v), 0.0, 1.0);

    // Base paint: per-facet tone variation, gradient toward the deep
    // colour at the feet, internal ripples from quality High.
    var albedo = mix(liquid.secondary.rgb, liquid.primary.rgb, 0.72 + 0.28 * smoothstep(0.0, 0.55, height));
    albedo *= 0.97 + 0.06 * facet;
    let speed = liquid.motion.x;
    let deform = liquid.motion.y;
    var ripple_line = 0.0;
    if quality >= 2.0 {
        let ph = t * speed * 6.2831;
        let w1 = sin(bp.y * 0.85 - ph * 1.3 + sin(bp.x * 0.55 + ph * 0.7) * 1.6);
        let w2 = sin(bp.z * 0.9 + bp.x * 0.4 + ph * 0.9);
        let r = w1 * w2;
        albedo *= 1.0 + r * 0.07 * deform;
        ripple_line = smoothstep(0.86, 0.99, abs(r)) * 0.35 * deform;
    }

    // Toon lighting from the directional lights (with their shadows).
    let view_z = dot(vec4<f32>(
        view.view_from_world[0].z,
        view.view_from_world[1].z,
        view.view_from_world[2].z,
        view.view_from_world[3].z
    ), mesh.world_position);
    let band_count = max(liquid.motion.z, 1.0);
    let rough = clamp(liquid.look.z, 0.05, 1.0);
    var diffuse = vec3<f32>(0.0);
    var spec = vec3<f32>(0.0);
    var lit_amount = 0.0;
    for (var i: u32 = 0u; i < lights.n_directional_lights; i = i + 1u) {
        let dl = lights.directional_lights[i];
        let l = dl.direction_to_light;
        var shadow = 1.0;
        if (pbr_input.flags & MESH_FLAGS_SHADOW_RECEIVER_BIT) != 0u
            && (dl.flags & DIRECTIONAL_LIGHT_FLAGS_SHADOWS_ENABLED_BIT) != 0u {
            shadow = fetch_directional_shadow(i, mesh.world_position, pbr_input.world_normal, view_z, mesh.position.xy);
        }
        let wrap = clamp(dot(n, l) * 0.6 + 0.4, 0.0, 1.0) * mix(0.3, 1.0, shadow);
        let b = bands(wrap, band_count);
        diffuse += dl.color.rgb * b / PI;
        lit_amount = max(lit_amount, b);
        if quality >= 1.0 {
            let h = normalize(l + v);
            let edge = 1.0 - rough * 0.22;
            let hl = smoothstep(edge - 0.015, edge, dot(n, h)) * shadow;
            spec += dl.color.rgb / PI * hl * liquid.look.y * 0.55;
        }
    }
    // Shadowed sides lean toward the deep colour rather than grey.
    let tinted = mix(liquid.secondary.rgb * 0.85, albedo, clamp(lit_amount * 1.4, 0.0, 1.0));
    let exposure = view.exposure;
    var color = tinted * (diffuse + lights.ambient_color.rgb) * exposure;
    color += mix(vec3<f32>(1.0), liquid.contour.rgb, 0.35) * spec * exposure;

    // Light floor: characters never sink into black in unlit corners.
    color = max(color, albedo * 0.16);

    // Fresnel contour, independent of the lighting (always readable).
    let ci = liquid.look.x * (1.0 + region * 0.25);
    var rim = 0.0;
    if quality >= 1.0 {
        rim = smoothstep(0.38, 0.0, ndv);
    } else {
        rim = step(ndv, 0.22);
    }
    color = mix(color, liquid.contour.rgb * 0.9, clamp(rim * ci, 0.0, 0.85));

    color += liquid.contour.rgb * ripple_line * 0.25;
    if quality >= 3.0 {
        // Ultra: wet sheen sweeping down the body.
        let sweep = fract(height * 1.5 + t * speed * 0.6 + facet * 0.15);
        color += liquid.contour.rgb * smoothstep(0.96, 1.0, sweep) * 0.12 * deform;
    }

    // Hit flash and respawn shimmer (cosmetic, never transparent).
    color = mix(color, liquid.contour.rgb, liquid.state.z * 0.45);
    let rf = liquid.state.y;
    if rf >= 0.0 {
        let band_d = abs(height - rf);
        color += liquid.contour.rgb * smoothstep(0.06, 0.0, band_d) * 0.8;
    }
    color = mix(color, liquid.contour.rgb, melt_edge * 0.6);

    var out: FragmentOutput;
    out.color = main_pass_post_lighting_processing(pbr_input, vec4<f32>(color, liquid.look.w));
    return out;
}
