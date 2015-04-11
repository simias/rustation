//! Shader and program related-code

use std::ptr;

use gl;
use gl::types::{GLenum, GLint, GLuint};

use std::ffi::CString;

use gpu::opengl::error::check_for_errors;

pub fn compile_shader(src: &str, shader_type: GLenum) -> GLuint {
    let shader;

    unsafe {
        shader = gl::CreateShader(shader_type);
        // Attempt to compile the shader
        let c_str = CString::new(src.as_bytes()).unwrap();
        gl::ShaderSource(shader, 1, &c_str.as_ptr(), ptr::null());
        gl::CompileShader(shader);

        check_for_errors();

        // Extra bit of error checking in case we're not using a DEBUG
        // OpenGL context and check_for_errors can't do it properly:
        let mut status = gl::FALSE as GLint;
        gl::GetShaderiv(shader, gl::COMPILE_STATUS, &mut status);

        if status != (gl::TRUE as GLint) {
            panic!("Shader compilation failed!");
        }
    }

    shader
}

pub fn link_program(shaders: &[GLuint]) -> GLuint {
    let program;

    unsafe {
        program = gl::CreateProgram();

        for &shader in shaders {
            gl::AttachShader(program, shader);
        }

        gl::LinkProgram(program);

        check_for_errors();

        // Extra bit of error checking in case we're not using a DEBUG
        // OpenGL context and check_for_errors can't do it properly:
        let mut status = gl::FALSE as GLint;
        gl::GetProgramiv(program, gl::LINK_STATUS, &mut status);

        if status != (gl::TRUE as GLint) {
            panic!("OpenGL program linking failed!");
        }
    }

    program
}

/// Return the index of attribute `attr` in `program`. Panics if the
/// index isn't found.
pub fn find_program_attrib(program: GLuint, attr: &str) -> GLuint {
    let cstr = CString::new(attr).unwrap().as_ptr();

    let index = unsafe { gl::GetAttribLocation(program, cstr) };

    if index < 0 {
        panic!("Attribute \"{}\" not found in program", attr);
    }

    index as GLuint
}
