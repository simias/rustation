//! Check for error messages using GL_KHR_debug:
//! https://www.opengl.org/registry/specs/KHR/debug.txt
//!
//! Requires a debug OpenGL!

use gl;
use gl::types::{GLenum, GLchar, GLsizei};

use std::str;

/// Check for OpenGL errors using `gl::GetDebugMessageLog`. If a
/// severe error is encountered this function panics. If the OpenGL
/// context doesn't have the DEBUG attribute this *probably* won't do
/// anything.
pub fn check_for_errors() {
    let mut fatal = false;

    loop {
        let mut buffer = vec![0; 4096];

        let mut severity = 0;
        let mut source = 0;
        let mut message_size= 0;
        let mut mtype = 0;
        let mut id = 0;

        let count =
            unsafe {
                gl::GetDebugMessageLog(1,
                                       buffer.len() as GLsizei,
                                       &mut source,
                                       &mut mtype,
                                       &mut id,
                                       &mut severity,
                                       &mut message_size,
                                       buffer.as_mut_ptr() as *mut GLchar)
            };

        if count == 0 {
            // No messages left
            break;
        }

        buffer.truncate(message_size as usize);

        let message =
            match str::from_utf8(&buffer) {
                Ok(m) => m,
                Err(e) => panic!("Got invalid message: {}", e),
            };

        let source = DebugSource::from_raw(source);
        let severity = DebugSeverity::from_raw(severity);
        let mtype = DebugType::from_raw(mtype);

        println!("OpenGL [{:?}|{:?}|{:?}|0x{:x}] {}",
                 severity, source, mtype, id, message);

        if severity.is_fatal() {
            // Something is very wrong, don't die just yet in order to
            // display any additional error message
            fatal = true;
        }
    }

    if fatal {
        panic!("Fatal OpenGL error");
    }
}

#[derive(Debug,PartialEq,Eq,Clone,Copy)]
enum DebugSeverity {
    High,
    Medium,
    Low,
    Notification,
}

impl DebugSeverity {
    fn from_raw(raw: GLenum) -> DebugSeverity {
        match raw {
            gl::DEBUG_SEVERITY_HIGH         => DebugSeverity::High,
            gl::DEBUG_SEVERITY_MEDIUM       => DebugSeverity::Medium,
            gl::DEBUG_SEVERITY_LOW          => DebugSeverity::Low,
            gl::DEBUG_SEVERITY_NOTIFICATION => DebugSeverity::Notification,
            _ => unreachable!(),
        }
    }

    /// Return true if execution should stop when this level of
    /// severity is encountered
    fn is_fatal(self) -> bool {
        // Should we stop at low as well?
        self == DebugSeverity::High || self == DebugSeverity::Medium
    }
}

#[derive(Debug,PartialEq,Eq,Clone,Copy)]
enum DebugSource {
    Api,
    WindowSystem,
    ShaderCompiler,
    ThirdParty,
    Application,
    Other,
}

impl DebugSource {
    fn from_raw(raw: GLenum) -> DebugSource {
        match raw {
            gl::DEBUG_SOURCE_API             => DebugSource::Api,
            gl::DEBUG_SOURCE_WINDOW_SYSTEM   => DebugSource::WindowSystem,
            gl::DEBUG_SOURCE_SHADER_COMPILER => DebugSource::ShaderCompiler,
            gl::DEBUG_SOURCE_THIRD_PARTY     => DebugSource::ThirdParty,
            gl::DEBUG_SOURCE_APPLICATION     => DebugSource::Application,
            gl::DEBUG_SOURCE_OTHER           => DebugSource::Other,
            _ => unreachable!(),
        }
    }
}


#[derive(Debug,PartialEq,Eq,Clone,Copy)]
enum DebugType {
    Error,
    DeprecatedBehavior,
    UndefinedBehavior,
    Portability,
    Performance,
    Other,
    Marker,
    PushGroup,
    PopGroup,
}

impl DebugType {
    fn from_raw(raw: GLenum) -> DebugType {
        match raw {
            gl::DEBUG_TYPE_ERROR               => DebugType::Error,
            gl::DEBUG_TYPE_DEPRECATED_BEHAVIOR => DebugType::DeprecatedBehavior,
            gl::DEBUG_TYPE_UNDEFINED_BEHAVIOR  => DebugType::UndefinedBehavior,
            gl::DEBUG_TYPE_PORTABILITY         => DebugType::Portability,
            gl::DEBUG_TYPE_PERFORMANCE         => DebugType::Performance,
            gl::DEBUG_TYPE_OTHER               => DebugType::Other,
            gl::DEBUG_TYPE_MARKER              => DebugType::Marker,
            gl::DEBUG_TYPE_PUSH_GROUP          => DebugType::PushGroup,
            gl::DEBUG_TYPE_POP_GROUP           => DebugType::PopGroup,
            _ => unreachable!(),
        }
    }
}
