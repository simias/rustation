use sdl2;
use sdl2::video::GLProfile;

use glium_sdl2;

use glium::{Program, VertexBuffer, Frame, Surface, DrawParameters, Rect, Blend};
use glium::index;
use glium::uniforms::{UniformsStorage, EmptyUniforms};
use glium::program::ProgramCreationInput;

/// Maximum number of vertex that can be stored in an attribute
/// buffers
const VERTEX_BUFFER_LEN: u32 = 64 * 1024;

#[derive(Copy,Clone,Debug)]
pub struct Vertex {
    /// Position in PlayStation VRAM coordinates
    pub position: [i16; 2],
    /// RGB color, 8bits per component
    pub color: [u8; 3],
    /// Vertex alpha value, used for blending.
    ///
    /// XXX This is not accurate, we should implement blending
    /// ourselves taking the current semi-transparency mode into
    /// account. We should maybe store two variables, one with the
    /// source factor and one with the destination factor.
    pub alpha: f32,
}

implement_vertex!(Vertex, position, color, alpha);

impl Vertex {
    pub fn new(pos: [i16; 2],
               color: [u8; 3],
               semi_transparent: bool) -> Vertex {
        let alpha =
            if semi_transparent {
                0.5
            } else {
                1.0
            };

        Vertex {
            position: pos,
            color: color,
            alpha: alpha,
        }
    }
}

pub struct Renderer {
    /// Glium display
    window: glium_sdl2::SDL2Facade,
    /// Glium surface,
    target: Option<Frame>,
    /// Framebuffer horizontal resolution (native: 1024)
    fb_x_res: u16,
    /// Framebuffer vertical resolution (native: 512)
    fb_y_res: u16,
    /// OpenGL Program object
    program: Program,
    /// Permanent vertex buffer
    vertex_buffer: VertexBuffer<Vertex>,
    /// List of queued draw commands. Each command contains a
    /// primitive type (triangle or line) and a number of *vertices*
    /// to be drawn from the `vertex_buffer`.
    command_queue: Vec<(index::PrimitiveType, u32)>,
    /// Current draw command. Will be pushed onto the `command_queue`
    /// if a new command needs to be started.
    current_command: (index::PrimitiveType, u32),
    /// GLSL uniforms
    uniforms: UniformsStorage<'static, [i32; 2], EmptyUniforms>,
    /// Current draw offset
    offset: (i16, i16),
    /// Glium draw parameters
    params: DrawParameters<'static>,
    /// Current number or vertices in the buffers
    nvertices: u32,
}

impl Renderer {

    pub fn new(sdl_context: &sdl2::Sdl) -> Renderer {
        use glium_sdl2::DisplayBuild;
        // Native PSX VRAM resolution
        let fb_x_res = 1024u16;
        let fb_y_res = 512u16;

        let video_subsystem = sdl_context.video().unwrap();

        let gl_attr = video_subsystem.gl_attr();
        gl_attr.set_context_version(3, 3);
        gl_attr.set_context_profile(GLProfile::Core);

        // XXX Debug context is likely to be slower, we should make
        // that configurable at some point.
        gl_attr.set_context_flags().debug().set();

        let window =
            video_subsystem.window("Rustation",
                                   fb_x_res as u32, fb_y_res as u32)
                                    .position_centered()
                                    .build_glium()
                                    .ok().expect("Can't create SDL2 window");

        {
            let mut target = window.draw();
            target.clear_color(0.0, 0.0, 0.0, 1.0);
            target.finish().unwrap();
        }
        // "Slurp" the contents of the shader files. Note: this is a
        // compile-time thing.
        let vs_src = include_str!("vertex.glsl");
        let fs_src = include_str!("fragment.glsl");
        let prog_input = ProgramCreationInput::SourceCode {
            vertex_shader: &vs_src,
            tessellation_control_shader: None,
            tessellation_evaluation_shader: None,
            geometry_shader: None,
            fragment_shader: &fs_src,
            transform_feedback_varyings: None,
            // We do manual gamma correction
            outputs_srgb: true,
            uses_point_size: false,
        };

        let program = Program::new(&window, prog_input).unwrap();

        let vertex_buffer =
            VertexBuffer::empty_persistent(&window,
                                           VERTEX_BUFFER_LEN as usize).unwrap();

        let uniforms = uniform! {
            offset: [0; 2],
        };

        // In order to have the line size scale with the internal
        // resolution upscale we need to compute the upscaling ratio.
        //
        // XXX I only use the y scaling factor since I assume that
        // both dimensions are scaled by the same ratio. Otherwise
        // we'd have to change the line thickness depending on its
        // angle and that would be tricky.
        let scaling_factor = fb_y_res as f32 / 512.;

        let params = DrawParameters {
            // Default to full screen
            scissor: Some(Rect {
                left: 0,
                bottom: 0,
                width: fb_x_res as u32,
                height: fb_y_res as u32
            }),
            line_width: Some(scaling_factor),
            // XXX temporary hack for semi-transparency, use basic
            // alpha blending.
            blend: Blend::alpha_blending(),
            ..Default::default()
        };

        Renderer {
            target: Some(window.draw()),
            window: window,
            fb_x_res: fb_x_res,
            fb_y_res: fb_y_res,
            program: program,
            vertex_buffer: vertex_buffer,
            command_queue: Vec::new(),
            current_command: (index::PrimitiveType::TrianglesList, 0),
            uniforms: uniforms,
            offset: (0, 0),
            params: params,
            nvertices: 0,
        }
    }

    /// Add a triangle to the draw buffer
    pub fn push_triangle(&mut self, vertices: &[Vertex; 3]) {
        self.push_primitive(index::PrimitiveType::TrianglesList,
                            vertices);
    }

    /// Add a quad to the draw buffer
    pub fn push_quad(&mut self, vertices: &[Vertex; 4]) {
        self.push_triangle(&[vertices[0], vertices[1], vertices[2]]);
        self.push_triangle(&[vertices[1], vertices[2], vertices[3]]);
    }

    /// Add a line to the draw buffer
    pub fn push_line(&mut self, vertices: &[Vertex; 2]) {
        self.push_primitive(index::PrimitiveType::LinesList,
                            vertices);
    }

    /// Add a primitive to the draw buffer
    fn push_primitive(&mut self,
                      primitive_type: index::PrimitiveType,
                      vertices: &[Vertex]) {
        let primitive_vertices = vertices.len() as u32;

        // Make sure we have enough room left to queue the vertex. We
        // need to push two triangles to draw a quad, so 6 vertex
        if self.nvertices + primitive_vertices > VERTEX_BUFFER_LEN {
            // The vertex attribute buffers are full, force an early
            // draw
            self.draw();
        }

        let (mut cmd_type, mut cmd_len) = self.current_command;

        if primitive_type != cmd_type {
            // We have to change the primitive type. Push the current
            // command onto the queue and start a new one.
            if cmd_len > 0 {
                self.command_queue.push(self.current_command);
            }

            cmd_type = primitive_type;
            cmd_len = 0;
        }

        // Copy the vertices into the vertex buffer
        let start = self.nvertices as usize;
        let end = start + primitive_vertices as usize;

        let slice = self.vertex_buffer.slice(start..end).unwrap();
        slice.write(vertices);

        self.nvertices += primitive_vertices;
        self.current_command = (cmd_type, cmd_len + primitive_vertices);
    }

    /// Fill a rectangle in memory with the given color. This method
    /// ignores the mask bit, the drawing area and the drawing offset.
    pub fn fill_rect(&mut self,
                     color: [u8; 3],
                     top: u16, left: u16,
                     bottom: u16, right: u16) {
        // Flush any pending draw commands
        self.draw();

        // Save the current value of the scissor
        let scissor = self.params.scissor;

        // Disable the scissor and offset
        self.params.scissor = None;
        self.uniforms = uniform! {
            offset: [0; 2],
        };

        let top = top as i16;
        let left = left as i16;
        // Fill rect is inclusive
        let bottom = bottom as i16;
        let right = right as i16;

        // Draw a quad to fill the rectangle
        self.push_quad(&[
            Vertex::new([left, top], color, false),
            Vertex::new([right, top], color, false),
            Vertex::new([left, bottom], color, false),
            Vertex::new([right, bottom], color, false),
            ]);

        self.draw();

        // Restore previous scissor box and offset
        self.params.scissor = scissor;

        let (x, y) = self.offset;
        self.uniforms = uniform! {
            offset: [x as i32, y as i32],
        };
    }

    /// Set the value of the uniform draw offset
    pub fn set_draw_offset(&mut self, x: i16, y: i16) {
        // Force draw for the primitives with the current offset
        self.draw();

        self.offset = (x, y);

        self.uniforms = uniform! {
            offset : [x as i32, y as i32],
        }
    }

    /// Set the drawing area. Coordinates are offsets in the
    /// PlayStation VRAM
    pub fn set_drawing_area(&mut self,
                            left: u16, top: u16,
                            right: u16, bottom: u16) {
        // Render any pending primitives
        self.draw();

        let (left, top) = self.scale_coords(left, top);
        let (right, bottom) = self.scale_coords(right, bottom);

        if left > right || bottom > top {
            // XXX What should we do here? This happens often because
            // the drawing area is set in two successive calls to set
            // the top_left and then bottom_right so the intermediate
            // value is often wrong.
            self.params.scissor = Some(Rect {
                left: 0,
                bottom: 0,
                width: 0,
                height: 0,
            });
        } else {
            // Width and height are inclusive
            let width = right - left + 1;
            let height = top - bottom + 1;

            self.params.scissor = Some(Rect {
                left: left,
                bottom: bottom,
                width: width,
                height: height,
            });
        }
    }

    /// Draw the buffered commands and reset the buffers
    pub fn draw(&mut self) {
        let target = self.target.as_mut().unwrap();

        // Push the last pending command if needed
        let (_, cmd_len) = self.current_command;

        if cmd_len > 0 {
            self.command_queue.push(self.current_command);
        }

        let mut vertex_pos = 0;

        for &(cmd_type, cmd_len) in &self.command_queue {
            let start = vertex_pos;
            let end = start + cmd_len as usize;

            let vertices = self.vertex_buffer.slice(start..end).unwrap();

            target.draw(vertices,
                        &index::NoIndices(cmd_type),
                        &self.program,
                        &self.uniforms,
                        &self.params)
                .unwrap();

            vertex_pos = end;
        }

        // Reset the buffers
        self.nvertices = 0;
        self.command_queue.clear();
        self.current_command = (index::PrimitiveType::TrianglesList, 0);
    }

    /// Draw the buffered commands and display them
    pub fn display(&mut self) {
        self.draw();

        {
            let target = self.target.take().unwrap();
            target.finish().unwrap();
        }

        self.target = Some(self.window.draw());
    }

    /// Convert coordinates in the PlayStation framebuffer to
    /// coordinates in our potentially scaled OpenGL
    /// framebuffer. Coordinates are rounded to the nearest pixel.
    fn scale_coords(&self, x: u16, y: u16) -> (u32, u32) {
        // OpenGL has (0, 0) at the bottom left, the PSX at the top
        // left so we need to complement the y coordinate
        let y = !y & 0x1ff;

        let x = (x as u32 * self.fb_x_res as u32 + 512) / 1024;
        let y = (y as u32 * self.fb_y_res as u32 + 256) / 512;

        (x, y)
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        if let Some(frame) = self.target.take() {
            frame.finish().unwrap();
        }
    }
}
