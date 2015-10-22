use std::ptr;

use sdl2;
use sdl2::video::GLProfile;

use gl;
use gl::types::{GLint, GLuint, GLubyte, GLshort, GLsizei};

use self::error::check_for_errors;
use self::shader::{compile_shader, link_program};
use self::shader::{find_program_attrib, find_program_uniform};
use self::buffer::Buffer;

mod error;
mod shader;
mod buffer;

pub struct Renderer {
    /// SDL2 Window
    #[allow(dead_code)]
    window: sdl2::video::Window,
    /// OpenGL Context
    #[allow(dead_code)]
    gl_context: sdl2::video::GLContext,
    /// Framebuffer horizontal resolution (native: 1024)
    fb_x_res: u16,
    /// Framebuffer vertical resolution (native: 512)
    fb_y_res: u16,
    /// Vertex shader object
    vertex_shader: GLuint,
    /// Fragment shader object
    fragment_shader: GLuint,
    /// OpenGL Program object
    program: GLuint,
    /// OpenGL Vertex array object
    vertex_array_object: GLuint,
    /// Buffer containing the vertice positions
    positions: Buffer<Position>,
    /// Buffer containing the vertice colors
    colors: Buffer<Color>,
    /// Current number or vertices in the buffers
    nvertices: u32,
    /// Index of the "offset" shader uniform
    uniform_offset: GLint,
}

impl Renderer {

    pub fn new(sdl_context: &sdl2::Sdl) -> Renderer {
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
        gl_attr.set_multisample_buffers(1);
        gl_attr.set_multisample_samples(4);

        let window =
            video_subsystem.window("Rustation",
                                   fb_x_res as u32, fb_y_res as u32)
            .position_centered()
            .opengl()
            .build()
            .ok().expect("Can't create SDL2 window");

        let gl_context =
            window.gl_create_context()
            .ok().expect("Can't create GL context");

        gl::load_with(|s| video_subsystem.gl_get_proc_address(s) );

        // Clear the window
        unsafe {
            gl::ClearColor(0., 0., 0., 1.0);
            gl::Clear(gl::COLOR_BUFFER_BIT);

            // Enable scissor test
            gl::Enable(gl::SCISSOR_TEST);
            // Default to full screen
            gl::Scissor(0, 0, fb_x_res as GLint, fb_y_res as GLint);
        }
        window.gl_swap_window();

        // "Slurp" the contents of the shader files. Note: this is a
        // compile-time thing.
        let vs_src = include_str!("vertex.glsl");
        let fs_src = include_str!("fragment.glsl");

        // Compile our shaders...
        let vertex_shader   = compile_shader(vs_src, gl::VERTEX_SHADER);
        let fragment_shader = compile_shader(fs_src, gl::FRAGMENT_SHADER);
        // ... Link our program...
        let program = link_program(&[vertex_shader, fragment_shader]);
        // ... And use it.
        unsafe {
            gl::UseProgram(program);
        }

        // Generate our vertex attribute object that will hold our
        // vertex attributes
        let mut vao = 0;
        unsafe {
            gl::GenVertexArrays(1, &mut vao);
            // Bind our VAO
            gl::BindVertexArray(vao);
        }

        // Setup the "position" attribute. First we create the buffer
        // holding the positions (this call also binds it)
        let positions = Buffer::new();

        unsafe {
            // Then we retreive the index for the attribute in the
            // shader
            let index = find_program_attrib(program, "vertex_position");

            // Enable it
            gl::EnableVertexAttribArray(index);

            // Link the buffer and the index: 2 GLshort attributes,
            // not normalized. That should send the data untouched to
            // the vertex shader.
            gl::VertexAttribIPointer(index, 2, gl::SHORT, 0, ptr::null());
        }

        check_for_errors();

        // Setup the "color" attribute and bind it
        let colors = Buffer::new();

        unsafe {
            let index = find_program_attrib(program, "vertex_color");
            gl::EnableVertexAttribArray(index);

            // Link the buffer and the index: 3 GLByte attributes, normalized.
            gl::VertexAttribPointer(index,
                                     3,
                                     gl::UNSIGNED_BYTE,
                                     gl::TRUE,
                                     0,
                                     ptr::null());
        }

        check_for_errors();

        // Retreive and initialize the draw offset
        let uniform_offset = find_program_uniform(program, "offset");
        unsafe {
            gl::Uniform2i(uniform_offset, 0, 0);
        }

        Renderer {
            window: window,
            gl_context: gl_context,
            fb_x_res: fb_x_res,
            fb_y_res: fb_y_res,
            vertex_shader: vertex_shader,
            fragment_shader: fragment_shader,
            program: program,
            vertex_array_object: vao,
            positions: positions,
            colors: colors,
            nvertices: 0,
            uniform_offset: uniform_offset,
        }
    }

    /// Add a triangle to the draw buffer
    pub fn push_triangle(&mut self,
                         positions: [Position; 3],
                         colors:    [Color; 3]) {

        // Make sure we have enough room left to queue the vertex
        if self.nvertices + 3 > VERTEX_BUFFER_LEN {
            println!("Vertex attribute buffers full, forcing draw");
            self.draw();
        }

        for i in 0..3 {
            // Push
            self.positions.set(self.nvertices, positions[i]);
            self.colors.set(self.nvertices, colors[i]);
            self.nvertices += 1;
        }
    }

    /// Add a quad to the draw buffer
    pub fn push_quad(&mut self,
                     positions: [Position; 4],
                     colors:    [Color; 4]) {

        // Make sure we have enough room left to queue the vertex. We
        // need to push two triangles to draw a quad, so 6 vertex
        if self.nvertices + 6 > VERTEX_BUFFER_LEN {
            // The vertex attribute buffers are full, force an early
            // draw
            self.draw();
        }

        // Push the first triangle
        for i in 0..3 {
            self.positions.set(self.nvertices, positions[i]);
            self.colors.set(self.nvertices, colors[i]);
            self.nvertices += 1;
        }

        // Push the 2nd triangle
        for i in 1..4 {
            self.positions.set(self.nvertices, positions[i]);
            self.colors.set(self.nvertices, colors[i]);
            self.nvertices += 1;
        }
    }

    /// Set the value of the uniform draw offset
    pub fn set_draw_offset(&mut self, x: i16, y: i16) {
        // Force draw for the primitives with the current offset
        self.draw();

        // Update the uniform value
        unsafe {
            gl::Uniform2i(self.uniform_offset, x as GLint, y as GLint);
        }
    }

    /// Set the drawing area. Coordinates are offsets in the
    /// PlayStation VRAM
    pub fn set_drawing_area(&mut self,
                            left: u16, top: u16,
                            right: u16, bottom: u16) {
        // Render any pending primitives
        self.draw();

        let fb_x_res = self.fb_x_res as GLint;
        let fb_y_res = self.fb_y_res as GLint;

        // Scale PlayStation VRAM coordinates if our framebuffer is
        // not at the native resolution
        let left = (left as GLint * fb_x_res) / 1024;
        let right = (right as GLint * fb_x_res) / 1024;

        let top = (top as GLint * fb_y_res) / 512;
        let bottom = (bottom as GLint * fb_y_res) / 512;

        // Width and height are inclusive
        let width = right - left + 1;
        let height = bottom - top + 1;

        // OpenGL has (0, 0) at the bottom left, the PSX at the top left
        let bottom = fb_y_res - bottom - 1;

        if width < 0 || height < 0 {
            // XXX What should we do here?
            println!("Unsupported drawing area: {}x{} [{}x{}->{}x{}]",
                     width, height,
                     left, top, right, bottom);
            unsafe {
                // Don't draw anything...
                gl::Scissor(0, 0, 0, 0);
            }
        } else {
            unsafe {
                gl::Scissor(left, bottom, width, height);
            }
        }
    }

    /// Draw the buffered commands and reset the buffers
    pub fn draw(&mut self) {
        unsafe {
            // Make sure all the data from the persistent mappings is
            // flushed to the buffer
            gl::MemoryBarrier(gl::CLIENT_MAPPED_BUFFER_BARRIER_BIT);

            gl::DrawArrays(gl::TRIANGLES, 0, self.nvertices as GLsizei);
        }

        // Wait for GPU to complete
        unsafe {
            let sync = gl::FenceSync(gl::SYNC_GPU_COMMANDS_COMPLETE, 0);

            loop {
                let r = gl::ClientWaitSync(sync,
                                           gl::SYNC_FLUSH_COMMANDS_BIT,
                                           10000000);

                if r == gl::ALREADY_SIGNALED || r == gl::CONDITION_SATISFIED {
                    // Drawing done
                    break;
                }
            }
        }

        // Reset the buffers
        self.nvertices = 0;

        check_for_errors();
    }

    /// Draw the buffered commands and display them
    pub fn display(&mut self) {
        self.draw();

        self.window.gl_swap_window();
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteVertexArrays(1, &self.vertex_array_object);
            gl::DeleteShader(self.vertex_shader);
            gl::DeleteShader(self.fragment_shader);
            gl::DeleteProgram(self.program);
        }
    }
}

/// Position in VRAM.
#[derive(Copy,Clone,Default,Debug)]
pub struct Position(pub GLshort, pub GLshort);

impl Position {
    /// Parse position from a GP0 parameter
    pub fn from_gp0(val: u32) -> Position {
        let x = val as i16;
        let y = (val >> 16) as i16;

        Position(x as GLshort, y as GLshort)
    }
}

/// RGB color
#[derive(Copy,Clone,Default,Debug)]
pub struct Color(pub GLubyte, pub GLubyte, pub GLubyte);

impl Color {
    /// Parse color from a GP0 parameter
    pub fn from_gp0(val: u32) -> Color {
        let r = val as u8;
        let g = (val >> 8) as u8;
        let b = (val >> 16) as u8;

        Color(r as GLubyte, g as GLubyte, b as GLubyte)
    }
}

/// Maximum number of vertex that can be stored in an attribute
/// buffers
const VERTEX_BUFFER_LEN: u32 = 64 * 1024;
