use super::GlyphInfo;

#[repr(C)]
#[derive(Copy, Clone)]
pub struct Vertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    pub color: [f32; 4],
    pub mode: f32,
    pub sdf_params: [f32; 3],
}

unsafe impl bytemuck::Zeroable for Vertex {}
unsafe impl bytemuck::Pod for Vertex {}

pub(crate) fn quad_vertices(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    u: f32,
    v: f32,
    uw: f32,
    vh: f32,
    color: [f32; 4],
    mode: f32,
) -> [Vertex; 6] {
    let x1 = x.round();
    let y1 = y.round();
    let x2 = (x + w).round();
    let y2 = (y + h).round();

    let sdf_params = [0.0, 0.0, 0.0];

    let v1 = Vertex {
        pos: [x1, y1],
        uv: [u, v],
        color,
        mode,
        sdf_params,
    };
    let v2 = Vertex {
        pos: [x2, y1],
        uv: [u + uw, v],
        color,
        mode,
        sdf_params,
    };
    let v3 = Vertex {
        pos: [x2, y2],
        uv: [u + uw, v + vh],
        color,
        mode,
        sdf_params,
    };
    let v4 = Vertex {
        pos: [x1, y2],
        uv: [u, v + vh],
        color,
        mode,
        sdf_params,
    };

    [v1, v2, v3, v1, v3, v4]
}

pub(crate) fn glyph_quad_rect(
    x: f32,
    y: f32,
    glyph: GlyphInfo,
    scale: f32,
) -> (f32, f32, f32, f32) {
    (
        x + glyph.offset_x * scale,
        y - glyph.offset_y * scale,
        glyph.width * scale,
        glyph.height * scale,
    )
}

pub(crate) fn squiggle_vertices(
    scale_factor: f32,
    x: f32,
    baseline_y: f32,
    w: f32,
    color: [f32; 4],
) -> [Vertex; 6] {
    let amplitude = 1.0 * scale_factor;
    let period = 0.6 / scale_factor;
    let thickness = 0.05 * scale_factor;

    let h = amplitude * 2.0 + thickness * 2.0 + 2.0;
    let y_center = baseline_y + amplitude + thickness;

    let x1 = x.round();
    let y1 = (y_center - h / 2.0).round();
    let x2 = (x + w).round();
    let y2 = (y_center + h / 2.0).round();

    let uv_x0 = 0.0;
    let uv_x1 = x2 - x1;
    let uv_y0 = -(h / 2.0);
    let uv_y1 = h / 2.0;

    let sdf_params = [amplitude, period, thickness];

    let v1 = Vertex {
        pos: [x1, y1],
        uv: [uv_x0, uv_y0],
        color,
        mode: 6.0,
        sdf_params,
    };
    let v2 = Vertex {
        pos: [x2, y1],
        uv: [uv_x1, uv_y0],
        color,
        mode: 6.0,
        sdf_params,
    };
    let v3 = Vertex {
        pos: [x2, y2],
        uv: [uv_x1, uv_y1],
        color,
        mode: 6.0,
        sdf_params,
    };
    let v4 = Vertex {
        pos: [x1, y2],
        uv: [uv_x0, uv_y1],
        color,
        mode: 6.0,
        sdf_params,
    };

    [v1, v2, v3, v1, v3, v4]
}

pub(crate) fn rounded_rect_gradient_vertices(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    r: f32,
    top_color: [f32; 4],
    bottom_color: [f32; 4],
) -> [Vertex; 6] {
    let w_round = w.round();
    let h_round = h.round();
    let x1 = x.round();
    let y1 = y.round();
    let x2 = (x + w).round();
    let y2 = (y + h).round();

    let hw = w_round / 2.0;
    let hh = h_round / 2.0;
    let sdf_params = [hw, hh, r];

    let v1 = Vertex {
        pos: [x1, y1],
        uv: [-hw, -hh],
        color: top_color,
        mode: 3.0,
        sdf_params,
    };
    let v2 = Vertex {
        pos: [x2, y1],
        uv: [hw, -hh],
        color: top_color,
        mode: 3.0,
        sdf_params,
    };
    let v3 = Vertex {
        pos: [x2, y2],
        uv: [hw, hh],
        color: bottom_color,
        mode: 3.0,
        sdf_params,
    };
    let v4 = Vertex {
        pos: [x1, y2],
        uv: [-hw, hh],
        color: bottom_color,
        mode: 3.0,
        sdf_params,
    };

    [v1, v2, v3, v1, v3, v4]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quad_vertices_round_positions_and_build_two_triangles() {
        let color = [1.0, 0.5, 0.25, 1.0];
        let vertices = quad_vertices(1.2, 2.6, 10.1, 20.2, 0.1, 0.2, 0.3, 0.4, color, 5.0);

        assert_eq!(vertices.len(), 6);
        assert_eq!(vertices[0].pos, [1.0, 3.0]);
        assert_eq!(vertices[1].pos, [11.0, 3.0]);
        assert_eq!(vertices[2].pos, [11.0, 23.0]);
        assert_eq!(vertices[3].pos, vertices[0].pos);
        assert_eq!(vertices[4].pos, vertices[2].pos);
        assert_eq!(vertices[5].pos, [1.0, 23.0]);
        assert_eq!(vertices[0].uv, [0.1, 0.2]);
        assert_eq!(vertices[2].uv, [0.4, 0.6]);
        assert_eq!(vertices[0].color, color);
        assert_eq!(vertices[0].mode, 5.0);
        assert_eq!(vertices[0].sdf_params, [0.0, 0.0, 0.0]);
    }

    #[test]
    fn glyph_quad_rect_keeps_scaled_edges_for_consistent_baseline_rounding() {
        let glyph = GlyphInfo {
            u: 0.0,
            v: 0.0,
            uw: 0.1,
            vh: 0.1,
            width: 7.25,
            height: 10.49,
            offset_x: 0.0,
            offset_y: 12.51,
            advance: 8.0,
            is_emoji: 0.0,
        };
        let (x, y, w, h) = glyph_quad_rect(10.0, 100.0, glyph, 1.0);
        let vertices = quad_vertices(
            x, y, w, h, glyph.u, glyph.v, glyph.uw, glyph.vh, [1.0; 4], 0.0,
        );

        assert_eq!(vertices[0].pos[1], 87.0);
        assert_eq!(vertices[2].pos[1], 98.0);
        assert_eq!(y.round() + h.round(), 97.0);
    }

    #[test]
    fn squiggle_vertices_encode_wave_params_and_extent() {
        let color = [0.2, 0.3, 0.4, 1.0];
        let vertices = squiggle_vertices(2.0, 10.4, 30.0, 50.2, color);

        assert_eq!(vertices.len(), 6);
        assert_eq!(vertices[0].pos[0], 10.0);
        assert_eq!(vertices[1].pos[0], 61.0);
        assert_eq!(vertices[0].color, color);
        assert_eq!(vertices[0].mode, 6.0);
        assert_eq!(vertices[0].sdf_params, [2.0, 0.3, 0.1]);
        assert_eq!(vertices[3].pos, vertices[0].pos);
        assert_eq!(vertices[4].pos, vertices[2].pos);
    }

    #[test]
    fn rounded_rect_gradient_vertices_keep_top_and_bottom_colors() {
        let top = [1.0, 0.0, 0.0, 1.0];
        let bottom = [0.0, 0.0, 1.0, 1.0];
        let vertices = rounded_rect_gradient_vertices(1.2, 2.2, 100.0, 50.0, 8.0, top, bottom);

        assert_eq!(vertices.len(), 6);
        assert_eq!(vertices[0].pos, [1.0, 2.0]);
        assert_eq!(vertices[2].pos, [101.0, 52.0]);
        assert_eq!(vertices[0].color, top);
        assert_eq!(vertices[1].color, top);
        assert_eq!(vertices[2].color, bottom);
        assert_eq!(vertices[0].mode, 3.0);
        assert_eq!(vertices[0].sdf_params, [50.0, 25.0, 8.0]);
        assert_eq!(vertices[0].uv, [-50.0, -25.0]);
        assert_eq!(vertices[2].uv, [50.0, 25.0]);
    }
}
