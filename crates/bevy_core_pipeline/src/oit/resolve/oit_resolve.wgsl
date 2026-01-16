#import bevy_render::view::View
#import bevy_pbr::mesh_view_types::{OitFragmentNode, OrderIndependentTransparencySettings}

@group(0) @binding(0) var<uniform> view: View;
@group(0) @binding(1) var<storage, read_write> nodes: array<OitFragmentNode>;
@group(0) @binding(2) var<storage, read_write> heads: array<u32>; // No need to be atomic
@group(0) @binding(3) var<storage, read_write> atomic_counter: u32; // No need to be atomic

#ifndef DEPTH_PREPASS
@group(1) @binding(0) var depth: texture_depth_2d;
#endif

struct OitFragment {
    color: u32,
    depth_alpha: u32,
}

struct FullscreenVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

const LINKED_LIST_END_SENTINEL: u32 = 0xFFFFFFFFu;

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    atomic_counter = 0u;
    let screen_index = u32(floor(in.position.x) + floor(in.position.y) * view.viewport.z);

    let head = heads[screen_index] - 1u;
    if head == LINKED_LIST_END_SENTINEL {
        // https://github.com/gfx-rs/wgpu/issues/4416
        if true {
            discard;
        }
        return vec4(0.0);
    } else {
#ifndef DEPTH_PREPASS
        // If depth prepass is disabled, load depth for manual depth testing.
        // This is necessary because early z doesn't seem to trigger in the transparent pass.
        // This should be done during the draw pass so those fragments simply don't exist in the list,
        // but this requires a bigger refactor
        let d = textureLoad(depth, vec2<i32>(in.position.xy), 0);
#else
        let d = 0.0;
#endif
        let color = resolve(head, d);
        heads[screen_index] = 0u; // LINKED_LIST_END_SENTINEL + 1u;
        return color;
    }
}

fn resolve(head: u32, opaque_depth: f32) -> vec4<f32> {
    var final_color = vec4<f32>(0.0);

    var packed_opaque_depth = bevy_core_pipeline::oit::pack_24bit_depth_8bit_alpha(opaque_depth, 1.0);

    var list_head = head;
    while list_head != LINKED_LIST_END_SENTINEL {
        // Find the nearest fragment
        var nearest_index = LINKED_LIST_END_SENTINEL;
        var nearest_prev = LINKED_LIST_END_SENTINEL;
        // 0 is the packed representation of depth = 0.0 and alpha = 0.0
        var nearest_depth_alpha = 0u;

        var prev = LINKED_LIST_END_SENTINEL;
        var current = list_head;

        while current != LINKED_LIST_END_SENTINEL {
            let node = nodes[current];
            let next = node.next;

#ifndef DEPTH_PREPASS
            // Optimization: to avoid keeping revisiting hidden fragments, remove them from the list while we find them
            if node.depth_alpha < packed_opaque_depth {
                if prev == LINKED_LIST_END_SENTINEL {
                    list_head = next;
                } else {
                    nodes[prev].next = next;
                }
                current = next;
                continue;
            }
#endif

            if node.depth_alpha > nearest_depth_alpha {
                nearest_index = current;
                nearest_prev = prev;
                nearest_depth_alpha = node.depth_alpha;
            }

            prev = current;
            current = next;
        }

        // This edge case can only happen when fragments are either all hidden or all infinitely far and transparent
        // The second case can't happen because we prune those fragments in the draw pass.
#ifndef DEPTH_PREPASS
        if nearest_index == LINKED_LIST_END_SENTINEL {
            break;
        }
#endif

        // Unlink the nearest fragment from the list
        let nearest_next = nodes[nearest_index].next;
        if nearest_prev == LINKED_LIST_END_SENTINEL {
            list_head = nearest_next;
        } else {
            nodes[nearest_prev].next = nearest_next;
        }

        // Blend the fragment
        let color = bevy_pbr::rgb9e5::rgb9e5_to_vec3_(nodes[nearest_index].color);
        let alpha = packed_depth_alpha_get_alpha(nearest_depth_alpha);
        var base_color = vec4(color.rgb * alpha, alpha);
        final_color = blend(final_color, base_color);

        // early out
        if final_color.a == 1.0 {
            break;
        }
    }

    return final_color;
}

// OVER operator using premultiplied alpha
// see: https://en.wikipedia.org/wiki/Alpha_compositing
fn blend(color_a: vec4<f32>, color_b: vec4<f32>) -> vec4<f32> {
    let final_color = color_a.rgb + (1.0 - color_a.a) * color_b.rgb;
    let alpha = color_a.a + (1.0 - color_a.a) * color_b.a;
    return vec4(final_color.rgb, alpha);
}

fn packed_depth_alpha_get_alpha(packed: u32) -> f32 {
    return bevy_core_pipeline::oit::unpack_24bit_depth_8bit_alpha(packed).y;
}

