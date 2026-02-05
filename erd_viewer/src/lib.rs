use erd_model::FOO;
use js_sys::{Float32Array, Uint32Array};
use wasm_bindgen::prelude::*;
use web_sys::{
    console, HtmlCanvasElement, WebGl2RenderingContext as GL, WebGlProgram, WebGlTexture,
};

#[wasm_bindgen]
pub struct Globe {
    gl: GL,
    program: WebGlProgram,
    sphere_vbo: web_sys::WebGlBuffer,
    sphere_ibo: web_sys::WebGlBuffer,
    index_count: i32,
    tex: WebGlTexture,
    dist: f32,        // globe distance
    orient: [f32; 4], // quaternion (x, y, z, w), identity = (0, 0, 0, 1)
}

#[wasm_bindgen]
impl Globe {
    #[wasm_bindgen(constructor)]
    pub fn new(canvas: HtmlCanvasElement) -> Result<Globe, JsValue> {
        console::log_2(&"Version:".into(), &FOO.into());
        let gl: GL = canvas
            .get_context("webgl2")?
            .ok_or("WebGL2 unavailable")?
            .dyn_into()?;

        // --- shaders ---
        let vert_src = r#"#version 300 es
        precision mediump float;

        in vec3 a_pos;
        in vec2 a_uv;

        uniform mat4 u_mvp;

        out vec2 v_uv;

        void main() {
            v_uv = a_uv;
            gl_Position = u_mvp * vec4(a_pos, 1.0);
        }"#;

        let frag_src = r#"#version 300 es
        precision mediump float;
        in vec2 v_uv;
        uniform sampler2D u_tex;
        out vec4 outColor;
        void main() {
            outColor = texture(u_tex, v_uv);
        }"#;

        let program = link_program(&gl, vert_src, frag_src)?;

        // --- sphere geometry ---
        let (verts, uvs, indices) = make_sphere(512, 1024); // higher-res
        let interleaved = interleave(&verts, &uvs);

        // vertex buffer
        let sphere_vbo = gl.create_buffer().unwrap();
        gl.bind_buffer(GL::ARRAY_BUFFER, Some(&sphere_vbo));
        unsafe {
            let arr = Float32Array::view(&interleaved);
            gl.buffer_data_with_array_buffer_view(GL::ARRAY_BUFFER, &arr, GL::STATIC_DRAW);
        }

        // index buffer (u32)
        let sphere_ibo = gl.create_buffer().unwrap();
        gl.bind_buffer(GL::ELEMENT_ARRAY_BUFFER, Some(&sphere_ibo));
        unsafe {
            let arr = Uint32Array::view(&indices);
            gl.buffer_data_with_array_buffer_view(GL::ELEMENT_ARRAY_BUFFER, &arr, GL::STATIC_DRAW);
        }

        // texture
        let tex = gl.create_texture().unwrap();
        gl.bind_texture(GL::TEXTURE_2D, Some(&tex));
        gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_WRAP_S, GL::CLAMP_TO_EDGE as i32);
        gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_WRAP_T, GL::CLAMP_TO_EDGE as i32);
        gl.tex_parameteri(
            GL::TEXTURE_2D,
            GL::TEXTURE_MIN_FILTER,
            GL::LINEAR_MIPMAP_LINEAR as i32,
        );
        gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_MAG_FILTER, GL::LINEAR as i32);

        Ok(Self {
            gl,
            program,
            sphere_vbo,
            sphere_ibo,
            index_count: indices.len() as i32,
            tex,
            dist: 2.2,
            orient: [0.0, 0.0, 0.0, 1.0],
        })
    }

    pub fn apply_drag(&mut self, dx: f32, dy: f32, scale: f32) {
        // Screen/world axes (camera looks down -Z, Y is up, X is right)
        let yaw_world = quat_from_axis_angle([0.0, 1.0, 0.0], dx * scale); // left/right drag → yaw
        let pitch_world = quat_from_axis_angle([1.0, 0.0, 0.0], dy * scale); // up/down drag → pitch

        // Pre-multiply so rotations are in world/screen space BEFORE current orientation
        let dq = quat_mul(yaw_world, pitch_world);
        self.orient = quat_normalize(quat_mul(dq, self.orient));
    }

    pub fn apply_twist(&mut self, delta: f32) {
        let rz = quat_from_axis_angle([0.0, 0.0, -1.0], delta); // try -Z; flip sign if it feels backward
        self.orient = quat_normalize(quat_mul(rz, self.orient)); // pre-multiply = screen/world space
    }

    pub fn set_distance(&mut self, d: f32) {
        self.dist = d.clamp(1.2, 10.0);
    }

    // Upload texture from JS
    pub fn set_image(&self, img: &web_sys::ImageBitmap) {
        let gl = &self.gl;
        gl.bind_texture(GL::TEXTURE_2D, Some(&self.tex));
        gl.pixel_storei(GL::UNPACK_FLIP_Y_WEBGL, 1);
        gl.tex_image_2d_with_u32_and_u32_and_image_bitmap(
            GL::TEXTURE_2D,
            0,
            GL::RGBA as i32,
            GL::RGBA,
            GL::UNSIGNED_BYTE,
            img,
        )
        .unwrap();
        gl.generate_mipmap(GL::TEXTURE_2D);
    }

    pub fn set_image_video(&self, video: &web_sys::HtmlVideoElement) {
        let gl = &self.gl;
        gl.bind_texture(GL::TEXTURE_2D, Some(&self.tex));
        gl.pixel_storei(GL::UNPACK_FLIP_Y_WEBGL, 0);
        gl.tex_image_2d_with_u32_and_u32_and_html_video_element(
            GL::TEXTURE_2D,
            0,
            GL::RGBA as i32,
            GL::RGBA,
            GL::UNSIGNED_BYTE,
            video,
        )
        .unwrap();
        gl.generate_mipmap(GL::TEXTURE_2D);
    }

    pub fn render(&mut self) {
        let gl = &self.gl;
        let w = gl.drawing_buffer_width();
        let h = gl.drawing_buffer_height();
        gl.viewport(0, 0, w, h);
        gl.clear_color(0.05, 0.1, 0.2, 1.0);
        gl.clear(GL::COLOR_BUFFER_BIT | GL::DEPTH_BUFFER_BIT);
        gl.enable(GL::DEPTH_TEST);
        gl.disable(GL::CULL_FACE); // TODO: remove
        gl.use_program(Some(&self.program));

        gl.bind_buffer(GL::ARRAY_BUFFER, Some(&self.sphere_vbo));
        gl.bind_buffer(GL::ELEMENT_ARRAY_BUFFER, Some(&self.sphere_ibo));

        let stride = (5 * std::mem::size_of::<f32>()) as i32;
        let pos_loc = gl.get_attrib_location(&self.program, "a_pos") as u32;
        gl.enable_vertex_attrib_array(pos_loc);
        gl.vertex_attrib_pointer_with_i32(pos_loc, 3, GL::FLOAT, false, stride, 0);

        let uv_loc = gl.get_attrib_location(&self.program, "a_uv") as u32;
        gl.enable_vertex_attrib_array(uv_loc);
        gl.vertex_attrib_pointer_with_i32(uv_loc, 2, GL::FLOAT, false, stride, (3 * 4) as i32);

        let aspect = w as f32 / h as f32;
        let proj = perspective(60f32.to_radians(), aspect, 0.01, 100.0);

        let rot = mat4_from_quat(self.orient);
        let tz = translate_z(-self.dist);
        let model = mul4x4(&tz, &rot); // model = Tz * R(q)

        // mvp = proj * model
        let mvp = mul4x4(&proj, &model);

        let loc = gl.get_uniform_location(&self.program, "u_mvp");
        if let Some(loc) = loc {
            gl.uniform_matrix4fv_with_f32_array(Some(&loc), false, &mvp);
        }

        gl.bind_texture(GL::TEXTURE_2D, Some(&self.tex));
        gl.draw_elements_with_i32(GL::TRIANGLES, self.index_count, GL::UNSIGNED_INT, 0);
    }
}

// =================== helpers ===================

fn link_program(gl: &GL, vs_src: &str, fs_src: &str) -> Result<WebGlProgram, JsValue> {
    let vs = compile_shader(gl, GL::VERTEX_SHADER, vs_src)?;
    let fs = compile_shader(gl, GL::FRAGMENT_SHADER, fs_src)?;
    let program = gl.create_program().unwrap();
    gl.attach_shader(&program, &vs);
    gl.attach_shader(&program, &fs);
    gl.link_program(&program);
    if !gl
        .get_program_parameter(&program, GL::LINK_STATUS)
        .as_bool()
        .unwrap_or(false)
    {
        return Err(JsValue::from_str(
            &gl.get_program_info_log(&program).unwrap_or_default(),
        ));
    }
    Ok(program)
}

fn compile_shader(gl: &GL, ty: u32, src: &str) -> Result<web_sys::WebGlShader, JsValue> {
    let shader = gl.create_shader(ty).unwrap();
    gl.shader_source(&shader, src);
    gl.compile_shader(&shader);
    if !gl
        .get_shader_parameter(&shader, GL::COMPILE_STATUS)
        .as_bool()
        .unwrap_or(false)
    {
        return Err(JsValue::from_str(
            &gl.get_shader_info_log(&shader).unwrap_or_default(),
        ));
    }
    Ok(shader)
}

fn interleave(verts: &[f32], uvs: &[f32]) -> Vec<f32> {
    let mut data = Vec::with_capacity(verts.len() / 3 * 5);
    for i in 0..verts.len() / 3 {
        data.extend_from_slice(&verts[i * 3..i * 3 + 3]);
        data.extend_from_slice(&uvs[i * 2..i * 2 + 2]);
    }
    data
}

fn make_sphere(lat: u32, lon: u32) -> (Vec<f32>, Vec<f32>, Vec<u32>) {
    let mut verts = vec![];
    let mut uvs = vec![];
    let mut idx = vec![];
    for y in 0..=lat {
        let v = y as f32 / lat as f32;
        let theta = v * std::f32::consts::PI;
        for x in 0..=lon {
            let u = x as f32 / lon as f32;
            let phi = -u * std::f32::consts::TAU;
            let sin_t = theta.sin();
            verts.extend_from_slice(&[phi.cos() * sin_t, theta.cos(), phi.sin() * sin_t]);
            uvs.extend_from_slice(&[u, v]);
        }
    }
    for y in 0..lat {
        for x in 0..lon {
            let i = y * (lon + 1) + x;
            let a = i;
            let b = i + lon + 1;
            idx.extend_from_slice(&[a, b, a + 1, b, b + 1, a + 1]);
        }
    }
    (verts, uvs, idx)
}

#[rustfmt::skip]
fn mul4x4(a: &[f32;16], b: &[f32;16]) -> [f32;16] {
    // column-major: out = a * b
    let mut out = [0.0;16];
    for col in 0..4 {
        for row in 0..4 {
            out[col*4 + row] =
                a[0*4 + row]*b[col*4 + 0] +
                a[1*4 + row]*b[col*4 + 1] +
                a[2*4 + row]*b[col*4 + 2] +
                a[3*4 + row]*b[col*4 + 3];
        }
    }
    out
}

#[rustfmt::skip]
fn perspective(fovy: f32, aspect: f32, znear: f32, zfar: f32) -> [f32;16] {
    let f = 1.0 / (0.5*fovy).tan();
    let nf = 1.0 / (znear - zfar);
    [
        f/aspect, 0.0, 0.0,  0.0,
        0.0,      f,   0.0,  0.0,
        0.0,      0.0,(zfar+znear)*nf, -1.0,
        0.0,      0.0,(2.0*zfar*znear)*nf, 0.0,
    ]
}

#[rustfmt::skip]
fn translate_z(z: f32) -> [f32;16] {
    [
        1.0,0.0,0.0,0.0,
        0.0,1.0,0.0,0.0,
        0.0,0.0,1.0,0.0,
        0.0,0.0,z,  1.0,
    ]
}

#[rustfmt::skip]
fn quat_mul(a: [f32;4], b: [f32;4]) -> [f32;4] {
    // (ax,ay,az,aw) * (bx,by,bz,bw)
    let (ax,ay,az,aw) = (a[0],a[1],a[2],a[3]);
    let (bx,by,bz,bw) = (b[0],b[1],b[2],b[3]);
    [
        aw*bx + ax*bw + ay*bz - az*by,
        aw*by - ax*bz + ay*bw + az*bx,
        aw*bz + ax*by - ay*bx + az*bw,
        aw*bw - ax*bx - ay*by - az*bz,
    ]
}

fn quat_normalize(q: [f32; 4]) -> [f32; 4] {
    let len = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    [q[0] / len, q[1] / len, q[2] / len, q[3] / len]
}

fn quat_from_axis_angle(axis: [f32; 3], angle: f32) -> [f32; 4] {
    let (sx, sy, sz) = (axis[0], axis[1], axis[2]);
    let n = (sx * sx + sy * sy + sz * sz).sqrt().max(1e-8);
    let (x, y, z) = (sx / n, sy / n, sz / n);
    let half = 0.5 * angle;
    let s = half.sin();
    [x * s, y * s, z * s, half.cos()]
}

#[rustfmt::skip]
fn mat4_from_quat(q: [f32;4]) -> [f32;16] {
    // column-major
    let (x,y,z,w) = (q[0],q[1],q[2],q[3]);
    // let (x2,y2,z2) = (x+x, y+y, z+z);
    let (xx,yy,zz) = (x*x, y*y, z*z);
    let (xy,xz,yz) = (x*y, x*z, y*z);
    let (wx,wy,wz) = (w*x, w*y, w*z);
    [
        1.0 - 2.0*(yy+zz),  2.0*(xy+ wz),     2.0*(xz - wy),     0.0,
        2.0*(xy - wz),      1.0 - 2.0*(xx+zz),2.0*(yz + wx),     0.0,
        2.0*(xz + wy),      2.0*(yz - wx),    1.0 - 2.0*(xx+yy), 0.0,
        0.0,                0.0,              0.0,               1.0,
    ]
}
