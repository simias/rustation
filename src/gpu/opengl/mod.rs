use std::ptr;

use sdl2;
use sdl2::video::{GLAttr, OPENGL, WindowPos};

use gl;
use gl::types::{GLint, GLuint, GLubyte, GLshort, GLsizei};

use libc::c_void;

use self::error::check_for_errors;
use self::shader::{compile_shader, link_program};
use self::shader::{find_program_attrib, find_program_uniform};
use self::buffer::Buffer;

mod error;
mod shader;
mod buffer;

pub struct Renderer {
    /// SDL2 context
    #[allow(dead_code)]
    sdl_context: sdl2::sdl::Sdl,
    /// SDL2 Window
    #[allow(dead_code)]
    window: sdl2::video::Window,
    /// OpenGL Context
    #[allow(dead_code)]
    gl_context: sdl2::video::GLContext,
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

    pub fn new() -> Renderer {
        let sdl_context = sdl2::init(::sdl2::INIT_VIDEO).unwrap();

        sdl2::video::gl_set_attribute(GLAttr::GLContextMajorVersion, 3);
        sdl2::video::gl_set_attribute(GLAttr::GLContextMinorVersion, 3);

        // XXX Debug context is likely to be slower, we should make
        // that configurable at some point.
        sdl2::video::gl_set_attribute(GLAttr::GLContextFlags,
                                      sdl2::video::GL_CONTEXT_DEBUG.bits());

        let window = sdl2::video::Window::new(&sdl_context,
                                              "PSX",
                                              WindowPos::PosCentered,
                                              WindowPos::PosCentered,
                                              1024, 512,
                                              OPENGL).unwrap();

        let gl_context = window.gl_create_context().unwrap();

        gl::load_with(|s|
                      sdl2::video::gl_get_proc_address(s).unwrap()
                      as *const c_void);

        // Clear the window
        unsafe {
            gl::ClearColor(0., 0., 0., 1.0);
            gl::Clear(gl::COLOR_BUFFER_BIT);
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

            // Link the buffer and the index: 3 GLByte attributes,
            // not normalized. That should send the data untouched to
            // the vertex shader.
            gl::VertexAttribIPointer(index,
                                     3,
                                     gl::UNSIGNED_BYTE,
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
            sdl_context: sdl_context,
            window: window,
            gl_context: gl_context,
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
