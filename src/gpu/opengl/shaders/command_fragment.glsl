#version 330 core

uniform sampler2D fb_texture;
// 0: Only draw opaque pixels, 1: only draw semi-transparent pixels
uniform int draw_semi_transparent;

in vec3 frag_shading_color;
// Texture page: base offset for texture lookup.
flat in uvec2 frag_texture_page;
// Texel coordinates within the page. Interpolated by OpenGL.
in vec2 frag_texture_coord;
// Clut coordinates in VRAM
flat in uvec2 frag_clut;
// 0: no texture, 1: raw-texture, 2: blended
flat in int frag_texture_blend_mode;
// 0: 16bpp (no clut), 1: 8bpp, 2: 4bpp
flat in int frag_depth_shift;
// 0: No dithering, 1: dithering enabled
flat in int frag_dither;
// 0: Opaque primitive, 1: semi-transparent primitive
flat in int frag_semi_transparent;

out vec4 frag_color;

const int BLEND_MODE_NO_TEXTURE    = 0;
const int BLEND_MODE_RAW_TEXTURE   = 1;
const int BLEND_MODE_TEXTURE_BLEND = 2;

// Read a 16bpp pixel in VRAM
vec4 vram_get_pixel(int x, int y) {
  return texelFetch(fb_texture, ivec2(x, 511 - y), 0);
}

// Take a normalized color and convert it into a 16bit 1555 ABGR
// integer in the format used internally by the Playstation GPU.
int rebuild_color(vec4 color) {
  int a = int(floor(color.a + 0.5));
  int r = int(floor(color.r * 31. + 0.5));
  int g = int(floor(color.g * 31. + 0.5));
  int b = int(floor(color.b * 31. + 0.5));

  return (a << 15) | (b << 10) | (g << 5) | r;
}

// PlayStation dithering pattern. The offset is selected based on the
// pixel position in VRAM, by blocks of 4x4 pixels. The value is added
// to the 8bit color components before they're truncated to 5 bits.
const int dither_pattern[16] =
  int[16](-4,  0, -3,  1,
           2, -2,  3, -1,
          -3,  1, -4,  0,
           3, -1,  2, -2);

void main() {

  vec4 color;

  if (frag_texture_blend_mode == BLEND_MODE_NO_TEXTURE) {
    if (frag_semi_transparent != draw_semi_transparent) {
      discard;
    }

    color = vec4(frag_shading_color, 0.0);
  } else {
    // Look up texture

    // Number of texel per VRAM 16bit "pixel" for the current depth
    int pix_per_hw = 1 << frag_depth_shift;

    // 8 and 4bpp textures contain several texels per 16bit VRAM
    // "pixel"
    float tex_x_float = frag_texture_coord.x / float(pix_per_hw);

    // Texture pages are limited to 256x256 pixels
    int tex_x = int(tex_x_float) & 0xff;
    int tex_y = int(frag_texture_coord.y) & 0xff;

    tex_x += int(frag_texture_page.x);
    tex_y += int(frag_texture_page.y);

    vec4 texel = vram_get_pixel(tex_x, tex_y);

    if (frag_depth_shift > 0) {
      // 8 and 4bpp textures are paletted so we need to lookup the
      // real color in the CLUT

      // First we need to convert the normalized color back to the
      // internal integer format since it's not a real color but 2 or
      // 4 CLUT indexes
      int icolor = rebuild_color(texel);

      // A little bitwise magic to get the index in the CLUT. 4bpp
      // textures have 4 texels per VRAM "pixel", 8bpp have 2. We need
      // to shift the current color to find the proper part of the
      // halfword and then mask away the rest.

      // Bits per pixel (4 or 8)
      int bpp = 16 >> frag_depth_shift;

      // 0xf for 4bpp, 0xff for 8bpp
      int mask = ((1 << bpp) - 1);

      // 0...3 for 4bpp, 1...2 for 8bpp
      int align = int(fract(tex_x_float) * pix_per_hw);

      // 0, 4, 8 or 12 for 4bpp, 0 or 8 for 8bpp
      int shift = (align * bpp);

      // Finally we have the index in the CLUT
      int index = (icolor >> shift) & mask;

      int clut_x = int(frag_clut.x) + index;
      int clut_y = int(frag_clut.y);

      // Look up the real color for the texel in the CLUT
      texel = vram_get_pixel(clut_x, clut_y);
    }

    int icolor = rebuild_color(texel);

    if (icolor == 0x0000) {
      // Fully transparent texel, discard
      discard;
    }

    int is_texel_semi_transparent = (icolor >> 15) & frag_semi_transparent;

    if (is_texel_semi_transparent != draw_semi_transparent) {
      // We're not drawing those texels in this pass, discard
      discard;
    }

    if (frag_texture_blend_mode == BLEND_MODE_RAW_TEXTURE) {
      color = texel;
    } else /* BLEND_MODE_TEXTURE_BLEND */ {
      // Blend the texel with the shading color. `frag_shading_color`
      // is multiplied by two so that it can be used to darken or
      // lighten the texture as needed. The result of the
      // multiplication should be saturated to 1.0 (0xff) but I think
      // OpenGL will take care of that since the output buffer holds
      // integers. The alpha/mask bit bit is taken directly from the
      // texture however.
      color = vec4(frag_shading_color * 2. * texel.rgb, texel.a);
    }
  }

  // Dithering
  int x_dither = int(gl_FragCoord.x) & 3;
  int y_dither = int(gl_FragCoord.y) & 3;

  // The multiplication by `frag_dither` will result in
  // `dither_offset` being 0 if dithering is disabled
  int dither_offset = dither_pattern[y_dither * 4 + x_dither] * frag_dither;

  float dither = float(dither_offset) / 255.;

  frag_color = color + vec4(dither, dither, dither, 0.);
}
