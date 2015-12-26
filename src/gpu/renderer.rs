pub trait Renderer {
    fn set_draw_offset(&mut self, x: i16, y: i16);
    fn set_draw_area(&mut self, top_left: (u16, u16), dimensions: (u16, u16));

    fn set_display_mode(&mut self,
                        top_left: (u16, u16),
                        resolution: (u16, u16),
                        depth_24bpp: bool);

    fn push_line(&mut self, &PrimitiveAttributes, &[Vertex; 2]);
    fn push_triangle(&mut self, &PrimitiveAttributes, &[Vertex; 3]);
    fn push_quad(&mut self, &PrimitiveAttributes, &[Vertex; 4]);

    fn fill_rect(&mut self,
                 color: [u8; 3],
                 top_left: (u16, u16),
                 dimensions: (u16, u16));

    fn load_image(&mut self,
                  top_left: (u16, u16),
                  dimensions: (u16, u16),
                  pixel_buffer: &[u16]);
}

pub struct Vertex {
    pub position: [i16; 2],
    pub color: [u8; 3],
    pub texture_coord: [u16; 2],
}

impl Vertex {
    pub fn new(position: [i16; 2], color: [u8; 3]) -> Vertex {
        Vertex {
            position: position,
            color: color,
            // Unused
            texture_coord: [0, 0],
        }
    }

    pub fn new_textured(position: [i16; 2],
                        color: [u8; 3],
                        texture_coord: [u16; 2]) -> Vertex {
        Vertex {
            position: position,
            color: color,
            texture_coord: texture_coord,
        }
    }
}

pub struct PrimitiveAttributes {
    /// If true then the equation defined by `semi_transparency_mode`
    /// is applied to semi-transparent pixels.
    pub semi_transparent: bool,
    /// When `semi_transparent` is true this defines the blending
    /// equation for semi-transparent pixels.
    pub semi_transparency_mode: SemiTransparencyMode,
    /// Blending equation, says if the primitive is simply gouraud
    /// shaded (or monochrome since it's just a special case of
    /// gouraud shading with the same color on all vertices),
    /// texture-mapped or a mix of both (texture blending).
    pub blend_mode: BlendMode,
    /// For textured primitives this contains the coordinates of the
    /// top-left coordinates of the texture page. Texture pages are
    /// always 256x256 pixels big and wrap around in case of
    /// out-of-bound access.
    pub texture_page: [u16; 2],
    /// The PlayStation GPU supports 4 and 8bpp paletted textures and
    /// 16bits "truecolor" textures.
    pub texture_depth: TextureDepth,
    /// For 4 and 8bpp paletted textures this contains the coordinates
    /// of the first entry of the palette. The next entries will be at
    /// x + 1, x + 2 etc...
    pub clut: [u16; 2],
    /// True if the primitive is dithered.
    pub dither: bool,
}

/// Primitive texturing methods
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BlendMode {
    /// No texture, used
    None,
    /// Raw texture
    Raw,
    /// Texture bledend with the monochrome/shading color
    Blended,
}

/// Semi-transparency modes supported by the PlayStation GPU
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SemiTransparencyMode {
    /// Source / 2 + destination / 2
    Average = 0,
    /// Source + destination
    Add = 1,
    /// Destination - source
    SubstractSource = 2,
    /// Destination + source / 4
    AddQuarterSource = 3,
}

/// Depth of the pixel values in a texture page
#[derive(Clone,Copy)]
pub enum TextureDepth {
    /// 4 bits per pixel, paletted
    T4Bpp = 0,
    /// 8 bits per pixel, paletted
    T8Bpp = 1,
    /// 16 bits per pixel, truecolor
    T16Bpp = 2,
}
